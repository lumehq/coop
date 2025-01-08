use gpui::prelude::FluentBuilder;
use gpui::{
    actions, img, Context, IntoElement, Model, ObjectFit, ParentElement, Render, Styled,
    StyledImage, ViewContext,
};
use nostr_sdk::prelude::*;
use ui::{
    button::{Button, ButtonVariants},
    popup_menu::PopupMenuExt,
    Icon, IconName, Sizable,
};

use crate::{constants::IMAGE_SERVICE, get_client};

actions!(account, [ToDo]);

pub struct Account {
    #[allow(dead_code)]
    public_key: PublicKey,
    metadata: Model<Option<Metadata>>,
}

impl Account {
    pub fn new(public_key: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let metadata = cx.new_model(|_| None);
        let async_metadata = metadata.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let client = get_client();
                let query = async_cx
                    .background_executor()
                    .spawn(async move { client.database().metadata(public_key).await })
                    .await;

                if let Ok(metadata) = query {
                    _ = async_cx.update_model(&async_metadata, |a, b| {
                        *a = metadata;
                        b.notify();
                    });
                };
            })
            .detach();

        Self {
            public_key,
            metadata,
        }
    }
}

impl Render for Account {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        Button::new("account")
            .small()
            .compact()
            .reverse()
            .ghost()
            .icon(Icon::new(IconName::ChevronDownSmall))
            .when_some(self.metadata.read(cx).as_ref(), |this, metadata| {
                this.map(|this| {
                    if let Some(picture) = metadata.picture.clone() {
                        this.flex_shrink_0().child(
                            img(format!("{}/?url={}&w=72&h=72&n=-1", IMAGE_SERVICE, picture))
                                .size_5()
                                .rounded_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    } else {
                        this.flex_shrink_0()
                            .child(img("brand/avatar.png").size_6().rounded_full())
                    }
                })
            })
            .popup_menu(move |this, _cx| {
                this.menu("Profile", Box::new(ToDo))
                    .menu("Contacts", Box::new(ToDo))
                    .menu("Settings", Box::new(ToDo))
                    .separator()
                    .menu("Change account", Box::new(ToDo))
            })
    }
}
