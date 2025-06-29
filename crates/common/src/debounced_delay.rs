use std::marker::PhantomData;
use std::time::Duration;

use futures::channel::oneshot;
use futures::FutureExt;
use gpui::{Context, Task, Window};

pub struct DebouncedDelay<E: 'static> {
    task: Option<Task<()>>,
    cancel_channel: Option<oneshot::Sender<()>>,
    _phantom_data: PhantomData<E>,
}

impl<E: 'static> Default for DebouncedDelay<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: 'static> DebouncedDelay<E> {
    pub fn new() -> Self {
        Self {
            task: None,
            cancel_channel: None,
            _phantom_data: PhantomData,
        }
    }

    pub fn fire_new<F>(
        &mut self,
        delay: Duration,
        window: &mut Window,
        cx: &mut Context<E>,
        func: F,
    ) where
        F: 'static + Send + FnOnce(&mut E, &mut Window, &mut Context<E>) -> Task<()>,
    {
        if let Some(channel) = self.cancel_channel.take() {
            _ = channel.send(());
        }

        let (sender, mut receiver) = oneshot::channel::<()>();
        self.cancel_channel = Some(sender);

        let previous_task = self.task.take();

        self.task = Some(cx.spawn_in(window, async move |entity, cx| {
            let mut timer = cx.background_executor().timer(delay).fuse();

            if let Some(previous_task) = previous_task {
                previous_task.await;
            }

            futures::select_biased! {
                _ = receiver => return,
                _ = timer => {}
            }

            if let Ok(Ok(task)) =
                cx.update(|window, cx| entity.update(cx, |project, cx| (func)(project, window, cx)))
            {
                task.await;
            }
        }));
    }
}
