use std::collections::HashSet;
use std::time::Duration;

pub use actions::*;
use chat::{Message, RenderedMessage, Room, RoomEvent, RoomKind, SendReport};
use common::{nip96_upload, RenderedTimestamp};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, list, px, red, relative, rems, svg, white, AnyElement, App, AppContext,
    ClipboardItem, Context, Entity, EventEmitter, Flatten, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ListAlignment, ListOffset, ListState, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, Render, RetainAllImageCache, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, Task, WeakEntity, Window,
};
use gpui_tokio::Tokio;
use indexset::{BTreeMap, BTreeSet};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use person::{Person, PersonRegistry};
use settings::AppSettings;
use smallvec::{smallvec, SmallVec};
use smol::fs;
use state::NostrRegistry;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonVariants};
use ui::context_menu::ContextMenuExt;
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputEvent, InputState, TextInput};
use ui::notification::Notification;
use ui::popup_menu::PopupMenuExt;
use ui::{
    h_flex, v_flex, ContextModal, Disableable, Icon, IconName, InteractiveElementExt, Sizable,
    StyledExt,
};

use crate::emoji::EmojiPicker;
use crate::text::RenderedText;

mod actions;
mod emoji;
mod text;

pub fn init(room: WeakEntity<Room>, window: &mut Window, cx: &mut App) -> Entity<ChatPanel> {
    cx.new(|cx| ChatPanel::new(room, window, cx))
}

/// Chat Panel
pub struct ChatPanel {
    id: SharedString,
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,

    /// Chat Room
    room: WeakEntity<Room>,

    /// Message list state
    list_state: ListState,

    /// All messages
    messages: BTreeSet<Message>,

    /// Mapping message ids to their rendered texts
    rendered_texts_by_id: BTreeMap<EventId, RenderedText>,

    /// Mapping message ids to their reports
    reports_by_id: BTreeMap<EventId, Vec<SendReport>>,

    /// Input state
    input: Entity<InputState>,

    /// Replies to
    replies_to: Entity<HashSet<EventId>>,

    /// Media Attachment
    attachments: Entity<Vec<Url>>,

    /// Upload state
    uploading: bool,

    /// Async operations
    tasks: SmallVec<[Task<()>; 2]>,

    /// Event subscriptions
    _subscriptions: SmallVec<[Subscription; 2]>,
}

impl ChatPanel {
    pub fn new(room: WeakEntity<Room>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Message...")
                .auto_grow(1, 20)
                .prevent_new_line_on_enter()
                .clean_on_escape()
        });

        let attachments = cx.new(|_| vec![]);
        let replies_to = cx.new(|_| HashSet::new());

        let messages = BTreeSet::from([Message::system()]);
        let list_state = ListState::new(messages.len(), ListAlignment::Bottom, px(1024.));

        let id: SharedString = room
            .read_with(cx, |this, _cx| this.id.to_string().into())
            .unwrap_or("Unknown".into());

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        if let Ok(connect) = room.read_with(cx, |this, cx| this.connect(cx)) {
            tasks.push(
                // Get messaging relays and encryption keys announcement for each member
                cx.background_spawn(async move {
                    if let Err(e) = connect.await {
                        log::error!("Failed to initialize room: {}", e);
                    }
                }),
            );
        };

        if let Ok(get_messages) = room.read_with(cx, |this, cx| this.get_messages(cx)) {
            tasks.push(
                // Load all messages belonging to this room
                cx.spawn_in(window, async move |this, cx| {
                    let result = get_messages.await;

                    this.update_in(cx, |this, window, cx| {
                        match result {
                            Ok(events) => {
                                this.insert_messages(&events, cx);
                            }
                            Err(e) => {
                                window.push_notification(e.to_string(), cx);
                            }
                        };
                    })
                    .ok();
                }),
            );
        }

        if let Some(room) = room.upgrade() {
            subscriptions.push(
                // Subscribe to room events
                cx.subscribe_in(&room, window, move |this, _room, event, window, cx| {
                    match event {
                        RoomEvent::Incoming(message) => {
                            this.insert_message(message, false, cx);
                        }
                        RoomEvent::Reload => {
                            this.load_messages(window, cx);
                        }
                    };
                }),
            );
        }

        subscriptions.push(
            // Subscribe to input events
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut Self, _input, event, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.send_message(window, cx);
                    };
                },
            ),
        );

        Self {
            id,
            messages,
            room,
            list_state,
            input,
            replies_to,
            attachments,
            rendered_texts_by_id: BTreeMap::new(),
            reports_by_id: BTreeMap::new(),
            uploading: false,
            image_cache: RetainAllImageCache::new(cx),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
            tasks,
        }
    }

    /// Load all messages belonging to this room
    fn load_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(get_messages) = self.room.read_with(cx, |this, cx| this.get_messages(cx)) {
            self.tasks.push(cx.spawn_in(window, async move |this, cx| {
                let result = get_messages.await;

                this.update_in(cx, |this, window, cx| {
                    match result {
                        Ok(events) => {
                            this.insert_messages(&events, cx);
                        }
                        Err(e) => {
                            window.push_notification(Notification::error(e.to_string()), cx);
                        }
                    };
                })
                .ok();
            }));
        }
    }

    /// Get user input content and merged all attachments
    fn input_content(&self, cx: &Context<Self>) -> String {
        // Get input's value
        let mut content = self.input.read(cx).value().trim().to_string();

        // Get all attaches and merge its with message
        let attachments = self.attachments.read(cx);

        if !attachments.is_empty() {
            let urls = attachments
                .iter()
                .map(|url| url.to_string())
                .collect_vec()
                .join("\n");

            if content.is_empty() {
                content = urls;
            } else {
                content = format!("{content}\n{urls}");
            }
        }

        content
    }

    /// Send a message to all members of the chat
    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Get the message which includes all attachments
        let content = self.input_content(cx);

        // Return if message is empty
        if content.trim().is_empty() {
            window.push_notification("Cannot send an empty message", cx);
            return;
        }

        // Get the current room entity
        let Some(room) = self.room.upgrade().map(|this| this.read(cx)) else {
            return;
        };

        // Get replies_to if it's present
        let replies: Vec<EventId> = self.replies_to.read(cx).iter().copied().collect();

        // Create a temporary message for optimistic update
        let rumor = room.create_message(&content, replies.as_ref(), cx);
        let rumor_id = rumor.id.unwrap();

        // Create a task for sending the message in the background
        let send_message = room.send_message(&rumor, cx);

        // Optimistically update message list
        cx.spawn_in(window, async move |this, cx| {
            // Wait for the delay
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;

            // Update the message list and reset the states
            this.update_in(cx, |this, window, cx| {
                this.remove_all_replies(cx);
                this.remove_all_attachments(cx);

                // Reset the input to its default state
                this.input.update(cx, |this, cx| {
                    this.set_loading(false, cx);
                    this.set_disabled(false, cx);
                    this.set_value("", window, cx);
                });

                // Update the message list
                this.insert_message(&rumor, true, cx);
            })
            .ok();
        })
        .detach();

        self.tasks.push(cx.spawn_in(window, async move |this, cx| {
            let result = send_message.await;

            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(reports) => {
                        // Update room's status
                        this.room
                            .update(cx, |this, cx| {
                                if this.kind != RoomKind::Ongoing {
                                    // Update the room kind to ongoing,
                                    // but keep the room kind if send failed
                                    if reports.iter().all(|r| !r.is_sent_success()) {
                                        this.kind = RoomKind::Ongoing;
                                        cx.notify();
                                    }
                                }
                            })
                            .ok();

                        // Insert the sent reports
                        this.reports_by_id.insert(rumor_id, reports);

                        cx.notify();
                    }
                    Err(e) => {
                        window.push_notification(e.to_string(), cx);
                    }
                }
            })
            .ok();
        }));
    }

    /// Insert a message into the chat panel
    fn insert_message<E>(&mut self, m: E, scroll: bool, cx: &mut Context<Self>)
    where
        E: Into<Message>,
    {
        let old_len = self.messages.len();

        // Extend the messages list with the new events
        if self.messages.insert(m.into()) {
            self.list_state.splice(old_len..old_len, 1);

            if scroll {
                self.list_state.scroll_to(ListOffset {
                    item_ix: self.list_state.item_count(),
                    offset_in_item: px(0.0),
                });
            }

            cx.notify();
        }
    }

    /// Convert and insert a vector of nostr events into the chat panel
    fn insert_messages(&mut self, events: &[UnsignedEvent], cx: &mut Context<Self>) {
        for event in events.iter() {
            // Bulk inserting messages, so no need to scroll to the latest message
            self.insert_message(event, false, cx);
        }
    }

    /// Check if a message failed to send by its ID
    fn is_sent_failed(&self, id: &EventId) -> bool {
        self.reports_by_id
            .get(id)
            .is_some_and(|reports| reports.iter().all(|r| !r.is_sent_success()))
    }

    /// Check if a message was sent successfully by its ID
    fn is_sent_success(&self, id: &EventId) -> Option<bool> {
        self.reports_by_id
            .get(id)
            .map(|reports| reports.iter().all(|r| r.is_sent_success()))
    }

    /// Get the sent reports for a message by its ID
    fn sent_reports(&self, id: &EventId) -> Option<&Vec<SendReport>> {
        self.reports_by_id.get(id)
    }

    /// Get a message by its ID
    fn message(&self, id: &EventId) -> Option<&RenderedMessage> {
        self.messages.iter().find_map(|msg| {
            if let Message::User(rendered) = msg {
                if &rendered.id == id {
                    return Some(rendered);
                }
            }
            None
        })
    }

    fn scroll_to(&self, id: EventId) {
        if let Some(ix) = self.messages.iter().position(|m| {
            if let Message::User(msg) = m {
                msg.id == id
            } else {
                false
            }
        }) {
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
                this.insert(text.id);
                cx.notify();
            });
        }
    }

    fn remove_reply(&mut self, id: &EventId, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            this.remove(id);
            cx.notify();
        });
    }

    fn remove_all_replies(&mut self, cx: &mut Context<Self>) {
        self.replies_to.update(cx, |this, cx| {
            this.clear();
            cx.notify();
        });
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let nostr = NostrRegistry::global(cx);
        let client = nostr.read(cx).client();

        // Get the user's configured NIP96 server
        let nip96_server = AppSettings::get_media_server(cx);

        let path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        cx.spawn_in(window, async move |this, cx| {
            let mut paths = path.await.ok()?.ok()??;
            let path = paths.pop()?;

            let upload = Tokio::spawn(cx, async move {
                let file = fs::read(path).await.ok()?;
                let url = nip96_upload(&client, &nip96_server, file).await.ok()?;

                Some(url)
            });

            if let Ok(task) = upload {
                this.update(cx, |this, cx| {
                    this.set_uploading(true, cx);
                })
                .ok();

                let result = Flatten::flatten(task.await.map_err(|e| e.into()));

                this.update_in(cx, |this, window, cx| {
                    match result {
                        Ok(Some(url)) => {
                            this.add_attachment(url, cx);
                            this.set_uploading(false, cx);
                        }
                        Ok(None) => {
                            this.set_uploading(false, cx);
                        }
                        Err(e) => {
                            window.push_notification(Notification::error(e.to_string()), cx);
                            this.set_uploading(false, cx);
                        }
                    };
                })
                .ok();
            }

            Some(())
        })
        .detach();
    }

    fn set_uploading(&mut self, uploading: bool, cx: &mut Context<Self>) {
        self.uploading = uploading;
        cx.notify();
    }

    fn add_attachment(&mut self, url: Url, cx: &mut Context<Self>) {
        self.attachments.update(cx, |this, cx| {
            this.push(url);
            cx.notify();
        });
    }

    fn remove_attachment(&mut self, url: &Url, _window: &mut Window, cx: &mut Context<Self>) {
        self.attachments.update(cx, |this, cx| {
            if let Some(ix) = this.iter().position(|this| this == url) {
                this.remove(ix);
                cx.notify();
            }
        });
    }

    fn remove_all_attachments(&mut self, cx: &mut Context<Self>) {
        self.attachments.update(cx, |this, cx| {
            this.clear();
            cx.notify();
        });
    }

    fn profile(&self, public_key: &PublicKey, cx: &Context<Self>) -> Person {
        let persons = PersonRegistry::global(cx);
        persons.read(cx).get(public_key, cx)
    }

    fn render_announcement(&self, ix: usize, cx: &Context<Self>) -> AnyElement {
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
            .child(SharedString::from(
                "This conversation is private. Only members can see each other's messages.",
            ))
            .into_any_element()
    }

    fn render_warning(&self, ix: usize, content: SharedString, cx: &Context<Self>) -> AnyElement {
        div()
            .id(ix)
            .relative()
            .w_full()
            .py_1()
            .px_3()
            .bg(cx.theme().warning_background)
            .child(
                h_flex()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().warning_foreground)
                    .child(Avatar::new("brand/system.png").size(rems(2.)))
                    .child(content),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(2.))
                    .h_full()
                    .bg(cx.theme().warning_active),
            )
            .into_any_element()
    }

    fn render_message(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(message) = self.messages.get_index(ix) {
            match message {
                Message::User(rendered) => {
                    let text = self
                        .rendered_texts_by_id
                        .entry(rendered.id)
                        .or_insert_with(|| RenderedText::new(&rendered.content, cx))
                        .element(ix.into(), window, cx);

                    self.render_text_message(ix, rendered, text, cx)
                }
                Message::Warning(content, _timestamp) => {
                    self.render_warning(ix, SharedString::from(content), cx)
                }
                Message::System(_timestamp) => self.render_announcement(ix, cx),
            }
        } else {
            self.render_warning(ix, SharedString::from("Message not found"), cx)
        }
    }

    fn render_text_message(
        &self,
        ix: usize,
        message: &RenderedMessage,
        text: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);

        let id = message.id;
        let author = self.profile(&message.author, cx);
        let public_key = author.public_key();

        let replies = message.replies_to.as_slice();
        let has_replies = !replies.is_empty();

        // Check if message is sent failed
        let is_sent_failed = self.is_sent_failed(&id);

        // Check if message is sent successfully
        let is_sent_success = self.is_sent_success(&id);

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
                        this.child(
                            div()
                                .id(SharedString::from(format!("{ix}-avatar")))
                                .child(Avatar::new(author.avatar()).size(rems(2.)))
                                .context_menu(move |this, _window, _cx| {
                                    let view = Box::new(OpenPublicKey(public_key));
                                    let copy = Box::new(CopyPublicKey(public_key));

                                    this.menu("View Profile", view)
                                        .menu("Copy Public Key", copy)
                                }),
                        )
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .flex_initial()
                            .overflow_hidden()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_sm()
                                    .text_color(cx.theme().text_placeholder)
                                    .child(
                                        div()
                                            .font_semibold()
                                            .text_color(cx.theme().text)
                                            .child(author.name()),
                                    )
                                    .child(message.created_at.to_human_time())
                                    .when_some(is_sent_success, |this, status| {
                                        this.when(status, |this| {
                                            this.child(self.render_message_sent(&id, cx))
                                        })
                                    }),
                            )
                            .when(has_replies, |this| {
                                this.children(self.render_message_replies(replies, cx))
                            })
                            .child(text)
                            .when(is_sent_failed, |this| {
                                this.child(self.render_message_reports(&id, cx))
                            }),
                    ),
            )
            .child(self.render_border(cx))
            .child(self.render_actions(&id, cx))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, _window, cx| {
                    this.copy_message(&id, cx);
                }),
            )
            .on_double_click(cx.listener(move |this, _, _window, cx| {
                this.reply_to(&id, cx);
            }))
            .hover(|this| this.bg(cx.theme().surface_background))
            .into_any_element()
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
                            .child(author.name()),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_ellipsis()
                            .line_clamp(1)
                            .child(SharedString::from(&message.content)),
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

    fn render_message_sent(&self, id: &EventId, _cx: &Context<Self>) -> impl IntoElement {
        div()
            .id(SharedString::from(id.to_hex()))
            .child(SharedString::from("• Sent"))
            .when_some(self.sent_reports(id).cloned(), |this, reports| {
                this.on_click(move |_e, window, cx| {
                    let reports = reports.clone();

                    window.open_modal(cx, move |this, _window, cx| {
                        this.show_close(true)
                            .title(SharedString::from("Sent Reports"))
                            .child(v_flex().pb_4().gap_4().children({
                                let mut items = Vec::with_capacity(reports.len());

                                for report in reports.iter() {
                                    items.push(Self::render_report(report, cx))
                                }

                                items
                            }))
                    });
                })
            })
    }

    fn render_message_reports(&self, id: &EventId, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .id(SharedString::from(id.to_hex()))
            .gap_0p5()
            .text_color(cx.theme().danger_foreground)
            .text_xs()
            .italic()
            .child(Icon::new(IconName::Info).xsmall())
            .child(SharedString::from(
                "Failed to send message. Click to see details.",
            ))
            .when_some(self.sent_reports(id).cloned(), |this, reports| {
                this.on_click(move |_e, window, cx| {
                    let reports = reports.clone();

                    window.open_modal(cx, move |this, _window, cx| {
                        this.show_close(true)
                            .title(SharedString::from("Sent Reports"))
                            .child(v_flex().gap_4().pb_4().w_full().children({
                                let mut items = Vec::with_capacity(reports.len());

                                for report in reports.iter() {
                                    items.push(Self::render_report(report, cx))
                                }

                                items
                            }))
                    });
                })
            })
    }

    fn render_report(report: &SendReport, cx: &App) -> impl IntoElement {
        let persons = PersonRegistry::global(cx);
        let profile = persons.read(cx).get(&report.receiver, cx);
        let name = profile.name();
        let avatar = profile.avatar();

        v_flex()
            .gap_2()
            .w_full()
            .child(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(SharedString::from("Sent to:"))
                    .child(
                        h_flex()
                            .gap_1()
                            .font_semibold()
                            .child(Avatar::new(avatar).size(rems(1.25)))
                            .child(name.clone()),
                    ),
            )
            .when(report.relays_not_found, |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .justify_center()
                        .p_2()
                        .h_20()
                        .w_full()
                        .text_sm()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger_background)
                        .text_color(cx.theme().danger_foreground)
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .text_center()
                                .child(SharedString::from("Messaging Relays not found")),
                        ),
                )
            })
            .when(report.device_not_found, |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .justify_center()
                        .p_2()
                        .h_20()
                        .w_full()
                        .text_sm()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger_background)
                        .text_color(cx.theme().danger_foreground)
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .text_center()
                                .child(SharedString::from("Encryption Key not found")),
                        ),
                )
            })
            .when_some(report.error.clone(), |this, error| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .justify_center()
                        .p_2()
                        .h_20()
                        .w_full()
                        .text_sm()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger_background)
                        .text_color(cx.theme().danger_foreground)
                        .child(div().flex_1().w_full().text_center().child(error)),
                )
            })
            .when_some(report.status.clone(), |this, output| {
                this.child(
                    v_flex()
                        .gap_2()
                        .w_full()
                        .children({
                            let mut items = Vec::with_capacity(output.failed.len());

                            for (url, msg) in output.failed.into_iter() {
                                items.push(
                                    v_flex()
                                        .gap_0p5()
                                        .py_1()
                                        .px_2()
                                        .w_full()
                                        .rounded(cx.theme().radius)
                                        .bg(cx.theme().elevated_surface_background)
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .line_height(relative(1.25))
                                                .child(SharedString::from(url.to_string())),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().danger_foreground)
                                                .line_height(relative(1.25))
                                                .child(SharedString::from(msg.to_string())),
                                        ),
                                )
                            }

                            items
                        })
                        .children({
                            let mut items = Vec::with_capacity(output.success.len());

                            for url in output.success.into_iter() {
                                items.push(
                                    v_flex()
                                        .gap_0p5()
                                        .py_1()
                                        .px_2()
                                        .w_full()
                                        .rounded(cx.theme().radius)
                                        .bg(cx.theme().elevated_surface_background)
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .line_height(relative(1.25))
                                                .child(SharedString::from(url.to_string())),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().secondary_foreground)
                                                .line_height(relative(1.25))
                                                .child(SharedString::from("Successfully")),
                                        ),
                                )
                            }

                            items
                        }),
                )
            })
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
        h_flex()
            .p_0p5()
            .gap_1()
            .invisible()
            .absolute()
            .right_4()
            .top_neg_2()
            .when(cx.theme().shadow, |this| this.shadow_sm())
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                Button::new("reply")
                    .icon(IconName::Reply)
                    .tooltip("Reply")
                    .small()
                    .ghost()
                    .on_click({
                        let id = id.to_owned();
                        cx.listener(move |this, _event, _window, cx| {
                            this.reply_to(&id, cx);
                        })
                    }),
            )
            .child(
                Button::new("copy")
                    .icon(IconName::Copy)
                    .tooltip("Copy")
                    .small()
                    .ghost()
                    .on_click({
                        let id = id.to_owned();
                        cx.listener(move |this, _event, _window, cx| {
                            this.copy_message(&id, cx);
                        })
                    }),
            )
            .child(div().flex_shrink_0().h_4().w_px().bg(cx.theme().border))
            .child(
                Button::new("seen-on")
                    .icon(IconName::Ellipsis)
                    .small()
                    .ghost()
                    .popup_menu({
                        let id = id.to_owned();
                        move |this, _, _| this.menu("Seen on", Box::new(SeenOn(id)))
                    }),
            )
            .group_hover("", |this| this.visible())
    }

    fn render_attachment(&self, url: &Url, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id(SharedString::from(url.to_string()))
            .relative()
            .w_16()
            .child(
                img(url.as_str())
                    .size_16()
                    .when(cx.theme().shadow, |this| this.shadow_lg())
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
            .on_click({
                let url = url.clone();
                cx.listener(move |this, _, window, cx| {
                    this.remove_attachment(&url, window, cx);
                })
            })
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
            let persons = PersonRegistry::global(cx);
            let profile = persons.read(cx).get(&text.author, cx);

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
                                .child(SharedString::from("Replying to:"))
                                .child(
                                    div()
                                        .text_color(cx.theme().text_accent)
                                        .child(profile.name()),
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
                        .child(SharedString::from(&text.content)),
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

impl Panel for ChatPanel {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.room
            .read_with(cx, |this, cx| {
                let label = this.display_name(cx);
                let url = this.display_image(cx);

                h_flex()
                    .gap_1p5()
                    .child(Avatar::new(url).size(rems(1.25)))
                    .child(label)
                    .into_any_element()
            })
            .unwrap_or(div().child("Unknown").into_any_element())
    }
}

impl EventEmitter<PanelEvent> for ChatPanel {}

impl Focusable for ChatPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .image_cache(self.image_cache.clone())
            .size_full()
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(|this, ix, window, cx| {
                        // Get and render message by index
                        this.render_message(ix, window, cx)
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
                        v_flex()
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
                                        h_flex()
                                            .gap_1()
                                            .text_color(cx.theme().text_muted)
                                            .child(
                                                Button::new("upload")
                                                    .icon(IconName::Upload)
                                                    .loading(self.uploading)
                                                    .disabled(self.uploading)
                                                    .ghost()
                                                    .large()
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.upload(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                EmojiPicker::new()
                                                    .target(self.input.downgrade())
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
