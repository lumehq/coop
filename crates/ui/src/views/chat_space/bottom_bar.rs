use client::NostrClient;
use components::theme::ActiveTheme;
use gpui::*;
use nostr_sdk::prelude::*;
use prelude::FluentBuilder;
use std::time::Duration;

use crate::state::AppState;

#[derive(Clone, IntoElement)]
struct Account {
    #[allow(dead_code)] // TODO: remove this
    public_key: PublicKey,
    metadata: Model<Option<Metadata>>,
}

impl Account {
    pub fn new(public_key: PublicKey, cx: &mut WindowContext) -> Self {
        let client = cx.global::<NostrClient>().client;

        let metadata = cx.new_model(|_| None);
        let async_metadata = metadata.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                match client
                    .fetch_metadata(public_key, Some(Duration::from_secs(2)))
                    .await
                {
                    Ok(metadata) => {
                        async_metadata
                            .update(&mut async_cx, |a, b| {
                                *a = Some(metadata);
                                b.notify()
                            })
                            .unwrap();
                    }
                    Err(_) => todo!(),
                }
            })
            .detach();

        Self {
            public_key,
            metadata,
        }
    }
}

impl RenderOnce for Account {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        match self.metadata.read(cx) {
            Some(metadata) => div()
                .w_8()
                .h_12()
                .px_1()
                .flex()
                .items_center()
                .justify_center()
                .border_b_2()
                .border_color(cx.theme().primary_active)
                .when_some(metadata.picture.clone(), |parent, picture| {
                    parent.child(
                        img(picture)
                            .size_6()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                }),
            None => div(), // TODO: add fallback image
        }
    }
}

pub struct BottomBar {
    accounts: Vec<Account>,
}

impl BottomBar {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> BottomBar {
        let state: Vec<PublicKey> = cx
            .global::<AppState>()
            .accounts
            .clone()
            .into_iter()
            .collect();

        let win_cx = cx.window_context();

        let accounts = state
            .into_iter()
            .map(|pk| Account::new(pk, win_cx))
            .collect::<Vec<_>>();

        BottomBar { accounts }
    }
}

impl Render for BottomBar {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .h_12()
            .px_3()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .children(self.accounts.clone())
    }
}
