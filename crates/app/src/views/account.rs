use crate::{get_client, states::app::AppRegistry};
use common::constants::IMAGE_SERVICE;
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

actions!(account, [ToDo]);

pub struct Account {
    public_key: PublicKey,
    metadata: Model<Option<Metadata>>,
}

impl Account {
    pub fn new(public_key: PublicKey, cx: &mut ViewContext<'_, Self>) -> Self {
        let metadata = cx.new_model(|_| None);
        let refreshs = cx.global_mut::<AppRegistry>().refreshs();

        if let Some(refreshs) = refreshs.upgrade() {
            cx.observe(&refreshs, |this, _, cx| {
                this.load_metadata(cx);
            })
            .detach();
        }

        Self {
            public_key,
            metadata,
        }
    }

    pub fn load_metadata(&self, cx: &mut ViewContext<Self>) {
        let mut async_cx = cx.to_async();
        let async_metadata = self.metadata.clone();

        cx.foreground_executor()
            .spawn({
                let client = get_client();
                let public_key = self.public_key;

                async move {
                    let metadata = async_cx
                        .background_executor()
                        .spawn(async move { client.database().metadata(public_key).await })
                        .await;

                    if let Ok(metadata) = metadata {
                        _ = async_cx.update_model(&async_metadata, |model, cx| {
                            *model = metadata;
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
    }
}

impl Render for Account {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        Button::new("account")
            .ghost()
            .xsmall()
            .reverse()
            .icon(Icon::new(IconName::ChevronDownSmall))
            .map(|this| {
                if let Some(metadata) = self.metadata.read(cx).as_ref() {
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
                                .child(img("brand/avatar.png").size_5().rounded_full())
                        }
                    })
                } else {
                    this.flex_shrink_0()
                        .child(img("brand/avatar.png").size_5().rounded_full())
                }
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
