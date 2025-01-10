use crate::{
    constants::IMAGE_SERVICE,
    get_client,
    states::{
        app::AppRegistry,
        chat::{ChatRegistry, Member, Room},
    },
    utils::{ago, room_hash},
    views::app::{AddPanel, PanelKind},
};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, percentage, Context, InteractiveElement, IntoElement, Model, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, View, ViewContext, VisualContext,
    WindowContext,
};
use nostr_sdk::prelude::*;
use std::sync::Arc;
use ui::{skeleton::Skeleton, theme::ActiveTheme, v_flex, Collapsible, Icon, IconName, StyledExt};

pub struct Inbox {
    label: SharedString,
    items: Model<Option<Vec<View<InboxListItem>>>>,
    is_loading: bool,
    is_collapsed: bool,
}

impl Inbox {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> Self {
        let items = cx.new_model(|_| None);
        let inbox = cx.global::<ChatRegistry>().inbox();

        if let Some(inbox) = inbox.upgrade() {
            cx.observe(&inbox, |this, model, cx| {
                this.load(model, cx);
            })
            .detach();
        }

        Self {
            items,
            label: "Inbox".into(),
            is_loading: true,
            is_collapsed: false,
        }
    }

    pub fn load(&mut self, model: Model<Vec<Event>>, cx: &mut ViewContext<Self>) {
        let events = model.read(cx).clone();
        let views: Vec<View<InboxListItem>> = events
            .into_iter()
            .map(|event| {
                cx.new_view(|cx| {
                    let view = InboxListItem::new(event, cx);
                    // Initial metadata
                    view.load_metadata(cx);

                    view
                })
            })
            .collect();

        self.items.update(cx, |model, cx| {
            *model = Some(views);
            cx.notify();
        });

        self.is_loading = false;
        cx.notify();
    }
}

impl Collapsible for Inbox {
    fn collapsed(mut self, collapsed: bool) -> Self {
        self.is_collapsed = collapsed;
        self
    }

    fn is_collapsed(&self) -> bool {
        self.is_collapsed
    }
}

impl Render for Inbox {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div();

        if self.is_loading {
            content = content.children((0..5).map(|_| {
                div()
                    .h_8()
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Skeleton::new().flex_shrink_0().size_6().rounded_full())
                    .child(Skeleton::new().w_20().h_3().rounded_sm())
            }))
        } else if let Some(items) = self.items.read(cx).as_ref() {
            content = content.children(items.clone())
        } else {
            // TODO: handle error
        }

        v_flex()
            .px_2()
            .gap_1()
            .child(
                div()
                    .id("inbox")
                    .h_7()
                    .px_1()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                    .hover(|this| this.bg(cx.theme().sidebar_accent.opacity(0.7)))
                    .on_click(cx.listener(move |view, _event, cx| {
                        view.is_collapsed = !view.is_collapsed;
                        cx.notify();
                    }))
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size_6()
                            .when(self.is_collapsed, |this| {
                                this.rotate(percentage(270. / 360.))
                            }),
                    )
                    .child(self.label.clone()),
            )
            .when(!self.is_collapsed, |this| this.child(content))
    }
}

struct InboxListItem {
    id: SharedString,
    created_at: Timestamp,
    owner: PublicKey,
    pubkeys: Vec<PublicKey>,
    members: Model<Vec<Member>>,
    is_group: bool,
}

impl InboxListItem {
    pub fn new(event: Event, cx: &mut ViewContext<'_, Self>) -> Self {
        let id = room_hash(&event.tags).to_string().into();
        let created_at = event.created_at;
        let owner = event.pubkey;

        let pubkeys: Vec<PublicKey> = event.tags.public_keys().copied().collect();
        let is_group = pubkeys.len() > 1;

        let members = cx.new_model(|_| Vec::new());
        let refreshs = cx.global_mut::<AppRegistry>().refreshs();

        if let Some(refreshs) = refreshs.upgrade() {
            cx.observe(&refreshs, |this, _, cx| {
                this.load_metadata(cx);
            })
            .detach();
        }

        Self {
            id,
            created_at,
            owner,
            pubkeys,
            members,
            is_group,
        }
    }

    pub fn load_metadata(&self, cx: &mut ViewContext<Self>) {
        let owner = self.owner;
        let public_keys = self.pubkeys.clone();
        let async_members = self.members.clone();

        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn({
                let client = get_client();

                async move {
                    let metadata: anyhow::Result<Vec<Member>, anyhow::Error> = async_cx
                        .background_executor()
                        .spawn(async move {
                            let mut result = Vec::new();

                            for public_key in public_keys.into_iter() {
                                let metadata = client.database().metadata(public_key).await?;
                                let profile = Member::new(public_key, metadata.unwrap_or_default());

                                result.push(profile);
                            }

                            let metadata = client.database().metadata(owner).await?;
                            let profile = Member::new(owner, metadata.unwrap_or_default());

                            result.push(profile);

                            Ok(result)
                        })
                        .await;

                    if let Ok(metadata) = metadata {
                        _ = async_cx.update_model(&async_members, |model, cx| {
                            *model = metadata;
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
    }

    pub fn action(&self, cx: &mut WindowContext<'_>) {
        let members = self.members.read(cx).clone();
        let room = Arc::new(Room::new(
            self.id.clone(),
            self.owner,
            self.created_at,
            None,
            members,
        ));

        cx.dispatch_action(Box::new(AddPanel {
            panel: PanelKind::Room(room),
            position: ui::dock::DockPlacement::Center,
        }))
    }
}

impl Render for InboxListItem {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let ago = ago(self.created_at.as_u64());
        let members = self.members.read(cx);

        let mut content = div()
            .font_medium()
            .text_color(cx.theme().sidebar_accent_foreground);

        if self.is_group {
            content = content
                .flex()
                .items_center()
                .gap_2()
                .child(img("brand/avatar.png").size_6().rounded_full())
                .map(|this| {
                    let names: Vec<String> = members
                        .iter()
                        .filter_map(|m| {
                            if m.public_key() != self.owner {
                                Some(m.name())
                            } else {
                                None
                            }
                        })
                        .collect();

                    this.child(names.join(", "))
                })
        } else {
            content = content.flex().items_center().gap_2().map(|this| {
                if let Some(member) = members.first() {
                    let mut child = this;

                    // Avatar
                    if let Some(picture) = member.metadata().picture.clone() {
                        child = child.child(
                            img(format!(
                                "{}/?url={}&w=72&h=72&fit=cover&mask=circle&n=-1",
                                IMAGE_SERVICE, picture
                            ))
                            .flex_shrink_0()
                            .size_6()
                            .rounded_full(),
                        );
                    } else {
                        child = child.child(
                            img("brand/avatar.png")
                                .flex_shrink_0()
                                .size_6()
                                .rounded_full(),
                        );
                    }

                    // Display name
                    child = child.child(member.name());

                    child
                } else {
                    this.child(
                        img("brand/avatar.png")
                            .flex_shrink_0()
                            .size_6()
                            .rounded_full(),
                    )
                    .child("Unknown")
                }
            })
        }

        div()
            .id(self.id.clone())
            .h_8()
            .px_1()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .rounded_md()
            .hover(|this| {
                this.bg(cx.theme().sidebar_accent)
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
            .child(content)
            .child(
                div()
                    .child(ago)
                    .text_color(cx.theme().sidebar_accent_foreground.opacity(0.7)),
            )
            .on_click(cx.listener(|this, _, cx| {
                this.action(cx);
            }))
    }
}
