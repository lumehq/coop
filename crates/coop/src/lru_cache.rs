use std::{collections::HashMap, sync::Arc};

use futures::FutureExt;
use gpui::{
    hash, AnyImageCache, App, AppContext, Asset, AssetLogger, Context, ElementId, Entity,
    ImageAssetLoader, ImageCache, ImageCacheProvider, Window,
};

pub fn cache_provider(id: impl Into<ElementId>, max_items: usize) -> LruCacheProvider {
    LruCacheProvider {
        id: id.into(),
        max_items,
    }
}

pub struct LruCacheProvider {
    id: ElementId,
    max_items: usize,
}

impl ImageCacheProvider for LruCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> AnyImageCache {
        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<LruCache>, _>(global_id, |lru_cache, _window| {
                    let mut lru_cache =
                        lru_cache.unwrap_or_else(|| cx.new(|cx| LruCache::new(self.max_items, cx)));
                    if lru_cache.read(cx).max_items != self.max_items {
                        lru_cache = cx.new(|cx| LruCache::new(self.max_items, cx));
                    }
                    (lru_cache.clone(), lru_cache)
                })
            })
            .into()
    }
}

struct LruCache {
    max_items: usize,
    usages: Vec<u64>,
    cache: HashMap<u64, gpui::ImageCacheItem>,
}

impl LruCache {
    fn new(max_items: usize, cx: &mut Context<Self>) -> Self {
        cx.on_release(|simple_cache, cx| {
            for (_, mut item) in std::mem::take(&mut simple_cache.cache) {
                if let Some(Ok(image)) = item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();

        Self {
            max_items,
            usages: Vec::with_capacity(max_items),
            cache: HashMap::with_capacity(max_items),
        }
    }
}

impl ImageCache for LruCache {
    fn load(
        &mut self,
        resource: &gpui::Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>> {
        assert_eq!(self.usages.len(), self.cache.len());
        assert!(self.cache.len() <= self.max_items);

        let hash = hash(resource);

        if let Some(item) = self.cache.get_mut(&hash) {
            let current_ix = self
                .usages
                .iter()
                .position(|item| *item == hash)
                .expect("cache and usages must stay in sync");
            self.usages.remove(current_ix);
            self.usages.insert(0, hash);

            return item.get();
        }

        let fut = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(fut).shared();
        if self.usages.len() == self.max_items {
            let oldest = self.usages.pop().unwrap();
            let mut image = self
                .cache
                .remove(&oldest)
                .expect("cache and usages must be in sync");
            if let Some(Ok(image)) = image.get() {
                cx.drop_image(image, Some(window));
            }
        }
        self.cache
            .insert(hash, gpui::ImageCacheItem::Loading(task.clone()));
        self.usages.insert(0, hash);

        let entity = window.current_view();
        window
            .spawn(cx, {
                async move |cx| {
                    _ = task.await;
                    cx.on_next_frame(move |_, cx| {
                        cx.notify(entity);
                    });
                }
            })
            .detach();

        None
    }
}
