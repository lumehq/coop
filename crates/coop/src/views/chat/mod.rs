use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use anyhow::anyhow;
use common::display::DisplayProfile;
use common::nip96::nip96_upload;
use global::nostr_client;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, list, px, red, relative, rems, svg, white, Action, AnyElement, App, AppContext,
    ClipboardItem, Context, Div, Element, Entity, EventEmitter, Flatten, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ListAlignment, ListState, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, Render, RetainAllImageCache, SharedString, Stateful,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use identity::Identity;
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::message::RenderedMessage;
use registry::room::{Room, RoomKind, RoomSignal, SendReport};
use registry::Registry;
use serde::Deserialize;
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::fs;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::emoji_picker::EmojiPicker;
use ui::input::{InputEvent, InputState, TextInput};
use ui::modal::ModalButtonProps;
use ui::notification::Notification;
use ui::popup_menu::PopupMenu;
use ui::text::RenderedText;
use ui::{
    v_flex, ContextModal, Disableable, Icon, IconName, InteractiveElementExt, Sizable, StyledExt,
};

mod subject;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = chat, no_json)]
pub struct ChangeSubject(pub String);

pub fn init(room: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Chat> {
    cx.new(|cx| Chat::new(room, window, cx))
}

pub struct Chat {
    // Panel
    id: SharedString,
    focus_handle: FocusHandle,
    // Chat Room
    room: Entity<Room>,
    list_state: ListState,
    messages: BTreeSet<RenderedMessage>,
    rendered_texts_by_id: HashMap<EventId, RenderedText>,
    reports_by_id: HashMap<EventId, Vec<SendReport>>,
    // New Message
    input: Entity<InputState>,
    replies_to: Entity<Vec<EventId>>,
    sending: bool,
    // Media Attachment
    attachments: Entity<Vec<Url>>,
    uploading: bool,
    // System
    image_cache: Entity<RetainAllImageCache>,
    #[allow(dead_code)]
    subscriptions: SmallVec<[Subscription; 3]>,
}

impl Chat {
    pub fn new(room: Entity<Room>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let attachments = cx.new(|_| vec![]);
        let replies_to = cx.new(|_| vec![]);
        let list_state = ListState::new(1, ListAlignment::Bottom, px(1024.));

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("chat.placeholder"))
                .multi_line()
                .prevent_new_line_on_enter()
                .rows(1)
                .multi_line()
                .auto_grow(1, 20)
                .clean_on_escape()
        });

        let mut subscriptions = smallvec![];

        subscriptions.push(cx.on_release_in(window, move |this, _window, cx| {
            this.messages.clear();
            this.rendered_texts_by_id.clear();
            this.reports_by_id.clear();

            this.attachments.update(cx, |this, _cx| {
                this.clear();
            });

            this.replies_to.update(cx, |this, _cx| {
                this.clear();
            })
        }));

        subscriptions.push(cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _input, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        this.send_message(window, cx);
                    }
                    InputEvent::Change(_) => {
                        // this.mention_popup(text, input, cx);
                    }
                    _ => {}
                };
            },
        ));

        subscriptions.push(
            cx.subscribe_in(&room, window, move |this, _, signal, window, cx| {
                match signal {
                    RoomSignal::NewMessage((gift_id, event)) => {
                        match this.sending {
                            false => {
                                if !this.is_sent_message(gift_id) {
                                    this.insert_message(event, cx);
                                };
                            }
                            true => {
                                let gift_id = gift_id.to_owned();
                                let event = event.clone();

                                cx.spawn_in(window, async move |this, cx| {
                                    // Check if Coop is still sending messages
                                    loop {
                                        cx.background_executor()
                                            .timer(Duration::from_secs(2))
                                            .await;

                                        if let Ok(false) =
                                            this.read_with(cx, |this, _cx| this.sending)
                                        {
                                            this.update(cx, |this, cx| {
                                                if !this.is_sent_message(&gift_id) {
                                                    this.insert_message(event, cx);
                                                }
                                            })
                                            .ok();

                                            break;
                                        }
                                    }
                                })
                                .detach();
                            }
                        }
                    }
                    RoomSignal::Refresh => {
                        this.load_messages(window, cx);
                    }
                };
            }),
        );

        Self {
            id: room.read(cx).id.to_string().into(),
            image_cache: RetainAllImageCache::new(cx),
            focus_handle: cx.focus_handle(),
            uploading: false,
            sending: false,
            messages: BTreeSet::new(),
            rendered_texts_by_id: HashMap::new(),
            reports_by_id: HashMap::new(),
            room,
            list_state,
            input,
            replies_to,
            attachments,
            subscriptions,
        }
    }

    /// Load all messages belonging to this room
    pub(crate) fn load_messages(&self, window: &mut Window, cx: &mut Context<Self>) {
        let room = self.room.read(cx);
        let load_messages = room.load_messages(cx);

        cx.spawn_in(window, async move |this, cx| {
            match load_messages.await {
                Ok(events) => {
                    this.update(cx, |this, cx| {
                        this.insert_messages(events, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    })
                    .ok();
                }
            };
        })
        .detach();
    }

    #[allow(dead_code)]
    fn mention_popup(&mut self, _text: &str, _input: &Entity<InputState>, _cx: &mut Context<Self>) {
        // TODO: open mention popup at current cursor position
    }

    /// Get user input content and merged all attachments
    fn input_content(&self, cx: &Context<Self>) -> String {
        let mut content = self.input.read(cx).value().trim().to_string();

        // Get all attaches and merge its with message
        let attachments = self.attachments.read(cx);

        if !attachments.is_empty() {
            content = format!(
                "{}\n{}",
                content,
                attachments
                    .iter()
                    .map(|url| url.to_string())
                    .collect_vec()
                    .join("\n")
            )
        }

        content
    }

    fn is_sent_message(&self, gift_id: &EventId) -> bool {
        let sent_ids = self
            .reports_by_id
            .values()
            .flatten()
            .filter_map(|report| report.success.as_ref().map(|success| success.id()))
            .collect_vec();

        sent_ids.contains(&gift_id)
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Return if user is not logged in
        let Some(identity) = Identity::read_global(cx).public_key() else {
            return;
        };

        // Get the message which includes all attachments
        let content = self.input_content(cx);

        // Get the backup setting
        let backup = AppSettings::get_backup_messages(cx);

        // Return if message is empty
        if content.trim().is_empty() {
            window.push_notification(t!("chat.empty_message_error"), cx);
            return;
        }

        // Mark sending in progress
        self.set_sending(true, cx);

        // Temporary disable input
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        // Get replies_to if it's present
        let replies = self.replies_to.read(cx).clone();

        // Get the current room entity
        let room = self.room.read(cx);

        // Create a temporary message for optimistic update
        let temp_message = room.create_temp_message(identity, &content, replies.as_ref());
        let temp_id = temp_message.id.unwrap();

        // Create a task for sending the message in the background
        let send_message = room.send_in_background(&content, replies, backup, cx);

        // Optimistically update message list
        self.insert_message(temp_message, cx);

        // Remove all replies
        self.remove_all_replies(cx);

        // Reset the input state
        self.input.update(cx, |this, cx| {
            this.set_loading(false, cx);
            this.set_disabled(false, cx);
            this.set_value("", window, cx);
        });

        // Continue sending the message in the background
        cx.spawn_in(window, async move |this, cx| {
            match send_message.await {
                Ok(reports) => {
                    this.update(cx, |this, cx| {
                        // Don't change the room kind if send failed
                        this.room.update(cx, |this, cx| {
                            if this.kind != RoomKind::Ongoing {
                                this.kind = RoomKind::Ongoing;
                                cx.notify();
                            }
                        });
                        this.reports_by_id.insert(temp_id, reports);
                        this.sending = false;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        window.push_notification(e.to_string(), cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn set_sending(&mut self, sending: bool, cx: &mut Context<Self>) {
        self.sending = sending;
        cx.notify();
    }

    fn message(&self, id: &EventId) -> Option<&RenderedMessage> {
        self.messages.iter().find(|m| m.id == *id)
    }

    fn success_reports(&self, id: &EventId) -> Option<&Vec<SendReport>> {
        self.reports_by_id.get(id)
    }

    fn error_reports(&self, id: &EventId) -> Option<&Vec<SendReport>> {
        self.reports_by_id
            .get(id)
            .filter(|r| r.iter().any(|report| report.has_error()))
    }

    fn insert_message<E>(&mut self, event: E, cx: &mut Context<Self>)
    where
        E: Into<RenderedMessage>,
    {
        let old_len = self.messages.len();
        let new_len = 1;

        // Extend the messages list with the new events
        self.messages.insert(event.into());

        // Update list state with the new messages
        self.list_state.splice(old_len..old_len, new_len);

        cx.notify();
    }

    fn insert_messages<E>(&mut self, events: E, cx: &mut Context<Self>)
    where
        E: IntoIterator,
        E::Item: Into<RenderedMessage>,
    {
        let old_len = self.messages.len();
        let events: Vec<_> = events.into_iter().map(Into::into).collect();
        let new_len = events.len();

        // Extend the messages list with the new events
        self.messages.extend(events);

        // Update list state with the new messages
        self.list_state.splice(old_len..old_len, new_len);

        cx.notify();
    }

    fn profile(&self, public_key: &PublicKey, cx: &Context<Self>) -> Profile {
        let registry = Registry::read_global(cx);
        registry.get_person(public_key, cx)
    }

    fn scroll_to(&self, id: EventId) {
        if let Some(ix) = self.messages.iter().position(|m| m.id == id) {
            self.list_state.scroll_to_reveal_item(ix);
        }
    }

    fn copy_message(&self, id: &EventId, cx: &Context<Self>) {
        if let Some(message) = self.message(id) {
            cx.write_to_clipboard(ClipboardItem::new_string(message.content.to_string()));
        }
    }

    fn reply_to(&mut self, id: &EventId, cx: &mut Context<Self>) {
        if let Some(text) = self.message(id) {
            self.replies_to.update(cx, |this, cx| {
                this.push(text.id);
                cx.notify();
            });
        }
    }

    fn remove_reply(&mut self, id: &EventId, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            if let Some(ix) = this.iter().position(|this| this == id) {
                this.remove(ix);
                cx.notify();
            }
        });
    }

    fn remove_all_replies(&mut self, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            *this = vec![];
            cx.notify();
        });
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uploading {
            return;
        }
        // Block the upload button to until current task is resolved
        self.uploading(true, cx);

        // Get the user's configured NIP96 server
        let nip96_server = AppSettings::get_media_server(cx);

        // Open native file dialog
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        let task = Tokio::spawn(cx, async move {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    if let Some(path) = paths.pop() {
                        let file = fs::read(path).await?;
                        let url = nip96_upload(nostr_client(), &nip96_server, file).await?;

                        Ok(url)
                    } else {
                        Err(anyhow!("Path not found"))
                    }
                }
                Ok(None) => Err(anyhow!("User cancelled")),
                Err(e) => Err(anyhow!("File dialog error: {e}")),
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(task.await.map_err(|e| e.into())) {
                Ok(Ok(url)) => {
                    this.update(cx, |this, cx| {
                        this.add_attachment(url, cx);
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    log::warn!("User cancelled: {e}");
                    this.update(cx, |this, cx| {
                        this.uploading(false, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            window.push_notification(e.to_string(), cx);
                            this.uploading(false, cx);
                        })
                        .ok();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn add_attachment(&mut self, url: Url, cx: &mut Context<Self>) {
        self.attachments.update(cx, |this, cx| {
            this.push(url);
            cx.notify();
        });
        self.uploading(false, cx);
    }

    fn remove_attachment(&mut self, url: &Url, _window: &mut Window, cx: &mut Context<Self>) {
        self.attachments.update(cx, |this, cx| {
            if let Some(ix) = this.iter().position(|this| this == url) {
                this.remove(ix);
                cx.notify();
            }
        });
    }

    fn uploading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.uploading = status;
        cx.notify();
    }

    fn render_announcement(&mut self, ix: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        v_flex()
            .id(ix)
            .group("")
            .h_32()
            .w_full()
            .relative()
            .gap_3()
            .px_3()
            .py_2()
            .items_center()
            .justify_center()
            .text_center()
            .text_xs()
            .text_color(cx.theme().text_placeholder)
            .line_height(relative(1.3))
            .child(
                svg()
                    .path("brand/coop.svg")
                    .size_10()
                    .text_color(cx.theme().elevated_surface_background),
            )
            .child(shared_t!("chat.notice"))
    }

    fn render_message(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(message) = self.messages.iter().nth(ix) else {
            return div().id(ix);
        };

        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);

        let id = message.id;
        let replies = message.replies_to.as_slice();
        let author = self.profile(&message.author, cx);

        // Get or insert rendered text
        let rendered_text = self
            .rendered_texts_by_id
            .entry(id)
            .or_insert_with(|| RenderedText::new(&message.content, cx))
            .element(ix.into(), window, cx);

        // Get all sent reports (only for messages sent by Coop)
        let success_reports = self.success_reports(&id);
        let error_reports = self.error_reports(&id);

        div()
            .id(ix)
            .group("")
            .relative()
            .w_full()
            .py_1()
            .px_3()
            .child(
                div()
                    .flex()
                    .gap_3()
                    .when(!hide_avatar, |this| {
                        this.child(Avatar::new(author.avatar_url(proxy)).size(rems(2.)))
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .flex_initial()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_sm()
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_color(cx.theme().text)
                                            .child(author.display_name()),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().text_placeholder)
                                            .child(message.ago()),
                                    )
                                    .when_some(success_reports, |this, reports| {
                                        this.child(
                                            Button::new("report")
                                                .icon(IconName::Check)
                                                .cta()
                                                .xsmall()
                                                .ghost_alt(),
                                        )
                                    }),
                            )
                            .when(!replies.is_empty(), |this| {
                                this.children(self.render_message_replies(replies, cx))
                            })
                            .child(rendered_text)
                            .when_some(error_reports, |this, reports| {
                                this.children(self.render_sent_error(reports, cx))
                            }),
                    ),
            )
            .child(self.render_border(cx))
            .child(self.render_actions(&id, cx))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_message(&id, cx);
                }),
            )
            .on_double_click(cx.listener({
                move |this, _event, _window, cx| {
                    this.reply_to(&id, cx);
                }
            }))
            .hover(|this| this.bg(cx.theme().surface_background))
    }

    fn render_message_replies(
        &self,
        replies: &[EventId],
        cx: &Context<Self>,
    ) -> impl IntoIterator<Item = impl IntoElement> {
        let mut items = Vec::with_capacity(replies.len());

        for (ix, id) in replies.iter().enumerate() {
            let Some(message) = self.message(id) else {
                continue;
            };
            let author = self.profile(&message.author, cx);

            items.push(
                div()
                    .id(ix)
                    .w_full()
                    .px_2()
                    .border_l_2()
                    .border_color(cx.theme().element_selected)
                    .text_sm()
                    .child(
                        div()
                            .text_color(cx.theme().text_accent)
                            .child(author.display_name()),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_ellipsis()
                            .line_clamp(1)
                            .child(message.content.clone()),
                    )
                    .hover(|this| this.bg(cx.theme().elevated_surface_background))
                    .on_click({
                        let id = *id;
                        cx.listener(move |this, _event, _window, _cx| {
                            this.scroll_to(id);
                        })
                    }),
            );
        }

        items
    }

    fn render_message_reports(
        &self,
        reports: &[SendReport],
        cx: &Context<Self>,
    ) -> impl IntoIterator<Item = impl IntoElement> {
        let mut items = Vec::with_capacity(reports.len());

        for (ix, report) in reports.iter().enumerate() {
            items.push(
                div()
                    .id(ix)
                    .text_color(cx.theme().icon_accent)
                    .child(Icon::new(IconName::Check).xsmall()),
            );
        }

        items
    }

    fn render_sent_error(
        &self,
        reports: &[SendReport],
        cx: &Context<Self>,
    ) -> impl IntoIterator<Item = impl IntoElement> {
        let mut items = Vec::with_capacity(reports.len());

        for (ix, report) in reports.iter().enumerate() {
            let author = self.profile(&report.receiver, cx);

            items.push(
                div()
                    .id(ix)
                    .italic()
                    .text_xs()
                    .text_color(cx.theme().danger_foreground)
                    .map(|this| {
                        if report.nip17_relays_not_found {
                            this.child(shared_t!("chat.nip17_not_found", u = author.display_name()))
                        } else {
                            this.when_some(report.error.clone(), |this, error| this.child(error))
                        }
                    }),
            );
        }

        items
    }

    fn render_border(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .group_hover("", |this| this.bg(cx.theme().element_active))
            .absolute()
            .left_0()
            .top_0()
            .w(px(2.))
            .h_full()
            .bg(cx.theme().border_transparent)
    }

    fn render_actions(&self, id: &EventId, cx: &Context<Self>) -> impl IntoElement {
        let id = id.to_owned();

        div()
            .group_hover("", |this| this.visible())
            .invisible()
            .absolute()
            .right_4()
            .top_neg_2()
            .shadow_sm()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .p_0p5()
            .flex()
            .gap_1()
            .children({
                vec![
                    Button::new("reply")
                        .icon(IconName::Reply)
                        .tooltip(t!("chat.reply_button"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.reply_to(&id, cx);
                        })),
                    Button::new("copy")
                        .icon(IconName::Copy)
                        .tooltip(t!("chat.copy_message_button"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.copy_message(&id, cx);
                        })),
                ]
            })
    }

    fn render_attachment(&self, url: &Url, cx: &Context<Self>) -> impl IntoElement {
        let url = url.clone();
        let path: SharedString = url.to_string().into();

        div()
            .id("")
            .relative()
            .w_16()
            .child(
                img(path.clone())
                    .size_16()
                    .shadow_lg()
                    .rounded(cx.theme().radius)
                    .object_fit(ObjectFit::ScaleDown),
            )
            .child(
                div()
                    .absolute()
                    .top_neg_2()
                    .right_neg_2()
                    .size_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(red())
                    .child(Icon::new(IconName::Close).size_2().text_color(white())),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.remove_attachment(&url, window, cx);
            }))
    }

    fn render_attachment_list(
        &self,
        _window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoIterator<Item = impl IntoElement> {
        let mut items = vec![];

        for url in self.attachments.read(cx).iter() {
            items.push(self.render_attachment(url, cx));
        }

        items
    }

    fn render_reply(&self, id: &EventId, cx: &Context<Self>) -> impl IntoElement {
        if let Some(text) = self.message(id) {
            let registry = Registry::read_global(cx);
            let profile = registry.get_person(&text.author, cx);

            div()
                .w_full()
                .pl_2()
                .border_l_2()
                .border_color(cx.theme().element_active)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().text_muted)
                                .child(SharedString::new(t!("chat.replying_to_label")))
                                .child(
                                    div()
                                        .text_color(cx.theme().text_accent)
                                        .child(profile.display_name()),
                                ),
                        )
                        .child(
                            Button::new("remove-reply")
                                .icon(IconName::Close)
                                .xsmall()
                                .ghost()
                                .on_click({
                                    let id = text.id;
                                    cx.listener(move |this, _, _, cx| {
                                        this.remove_reply(&id, cx);
                                    })
                                }),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .text_sm()
                        .text_ellipsis()
                        .line_clamp(1)
                        .child(text.content.clone()),
                )
        } else {
            div()
        }
    }

    fn render_reply_list(
        &self,
        _window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoIterator<Item = impl IntoElement> {
        let mut items = vec![];

        for id in self.replies_to.read(cx).iter() {
            items.push(self.render_reply(id, cx));
        }

        items
    }
}

impl Panel for Chat {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.room.read_with(cx, |this, cx| {
            let proxy = AppSettings::get_proxy_user_avatars(cx);
            let label = this.display_name(cx);
            let url = this.display_image(proxy, cx);

            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(Avatar::new(url).size(rems(1.25)))
                .child(label)
                .into_any()
        })
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, cx: &App) -> Vec<Button> {
        let room = self.room.downgrade();
        let subject = self
            .room
            .read(cx)
            .subject
            .as_ref()
            .map(|subject| subject.to_string());

        let button = Button::new("subject")
            .icon(IconName::EditFill)
            .tooltip(t!("chat.change_subject_button"))
            .on_click(move |_, window, cx| {
                let view = subject::init(subject.clone(), window, cx);
                let room = room.clone();
                let weak_view = view.downgrade();

                window.open_modal(cx, move |this, _window, _cx| {
                    let room = room.clone();
                    let weak_view = weak_view.clone();

                    this.confirm()
                        .title(SharedString::new(t!("chat.change_subject_modal_title")))
                        .child(view.clone())
                        .button_props(ModalButtonProps::default().ok_text(t!("common.change")))
                        .on_ok(move |_, _window, cx| {
                            if let Ok(subject) =
                                weak_view.read_with(cx, |this, cx| this.new_subject(cx))
                            {
                                room.update(cx, |this, cx| {
                                    this.subject = Some(subject);
                                    cx.notify();
                                })
                                .ok();
                            }
                            // true to close the modal
                            true
                        })
                });
            });

        vec![button]
    }
}

impl EventEmitter<PanelEvent> for Chat {}

impl Focusable for Chat {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Chat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .image_cache(self.image_cache.clone())
            .size_full()
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(move |this, ix, window, cx| {
                        if ix == 0 {
                            this.render_announcement(ix, cx).into_any_element()
                        } else {
                            this.render_message(ix, window, cx).into_any_element()
                        }
                    }),
                )
                .flex_1(),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .relative()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .children(self.render_attachment_list(window, cx))
                            .children(self.render_reply_list(window, cx))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_end()
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .text_color(cx.theme().text_muted)
                                            .child(
                                                Button::new("upload")
                                                    .icon(IconName::Upload)
                                                    .ghost()
                                                    .large()
                                                    .disabled(self.uploading)
                                                    .loading(self.uploading)
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.upload(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                EmojiPicker::new(self.input.downgrade())
                                                    .icon(IconName::EmojiFill)
                                                    .large(),
                                            ),
                                    )
                                    .child(TextInput::new(&self.input)),
                            ),
                    ),
            )
    }
}
