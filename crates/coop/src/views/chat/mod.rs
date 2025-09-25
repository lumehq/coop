use std::collections::HashMap;

use common::display::{RenderedProfile, RenderedTimestamp};
use common::nip96::nip96_upload;
use global::{app_state, nostr_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, list, px, red, relative, rems, svg, white, Action, AnyElement, App, AppContext,
    ClipboardItem, Context, Element, Entity, EventEmitter, Flatten, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ListAlignment, ListOffset, ListState, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, Render, RetainAllImageCache, SharedString, SharedUri,
    StatefulInteractiveElement, Styled, StyledImage, Subscription, Task, Window,
};
use gpui_tokio::Tokio;
use i18n::{shared_t, t};
use indexset::{BTreeMap, BTreeSet};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use registry::message::{Message, RenderedMessage};
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
use ui::popup_menu::{PopupMenu, PopupMenuExt};
use ui::text::RenderedText;
use ui::{
    h_flex, v_flex, ContextModal, Disableable, Icon, IconName, InteractiveElementExt, Sizable,
    StyledExt,
};

mod subject;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = chat, no_json)]
pub struct SeenOn(pub EventId);

pub fn init(room: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Chat> {
    cx.new(|cx| Chat::new(room, window, cx))
}

pub struct Chat {
    // Chat Room
    room: Entity<Room>,
    relays: Entity<HashMap<PublicKey, Vec<RelayUrl>>>,

    // Messages
    list_state: ListState,
    messages: BTreeSet<Message>,
    rendered_texts_by_id: BTreeMap<EventId, RenderedText>,
    reports_by_id: BTreeMap<EventId, Vec<SendReport>>,

    // New Message
    input: Entity<InputState>,
    replies_to: Entity<Vec<EventId>>,

    // Media Attachment
    attachments: Entity<Vec<Url>>,
    uploading: bool,

    // Panel
    id: SharedString,
    focus_handle: FocusHandle,
    image_cache: Entity<RetainAllImageCache>,

    _subscriptions: SmallVec<[Subscription; 4]>,
    _tasks: SmallVec<[Task<()>; 2]>,
}

impl Chat {
    pub fn new(room: Entity<Room>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let attachments = cx.new(|_| vec![]);
        let replies_to = cx.new(|_| vec![]);

        let relays = cx.new(|_| {
            let this: HashMap<PublicKey, Vec<RelayUrl>> = HashMap::new();
            this
        });

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("chat.placeholder"))
                .auto_grow(1, 20)
                .prevent_new_line_on_enter()
                .clean_on_escape()
        });

        let messages = BTreeSet::from([Message::system()]);
        let list_state = ListState::new(messages.len(), ListAlignment::Bottom, px(1024.));

        let connect_relays = room.read(cx).connect_relays(cx);
        let load_messages = room.read(cx).load_messages(cx);

        let mut subscriptions = smallvec![];
        let mut tasks = smallvec![];

        tasks.push(
            // Load all messages belonging to this room
            cx.spawn_in(window, async move |this, cx| {
                match connect_relays.await {
                    Ok(relays) => {
                        this.update(cx, |this, cx| {
                            this.relays.update(cx, |this, cx| {
                                *this = relays;
                                cx.notify();
                            });
                        })
                        .ok();
                    }
                    Err(e) => {
                        cx.update(|window, cx| {
                            window.push_notification(e.to_string(), cx);
                        })
                        .ok();
                    }
                };
            }),
        );

        tasks.push(
            // Load all messages belonging to this room
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
                            window.push_notification(e.to_string(), cx);
                        })
                        .ok();
                    }
                };
            }),
        );

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

        subscriptions.push(
            // Subscribe to room events
            cx.subscribe_in(&room, window, move |this, _, signal, window, cx| {
                match signal {
                    RoomSignal::NewMessage((gift_wrap_id, event)) => {
                        if !this.is_sent_by_coop(gift_wrap_id) {
                            this.insert_message(Message::user(event), false, cx);
                        }
                    }
                    RoomSignal::Refresh => {
                        this.load_messages(window, cx);
                    }
                };
            }),
        );

        subscriptions.push(
            // Observe the messaging relays of the room's members
            cx.observe_in(&relays, window, |this, entity, _window, cx| {
                for (public_key, urls) in entity.read(cx).clone().into_iter() {
                    if urls.is_empty() {
                        let profile = Registry::read_global(cx).get_person(&public_key, cx);
                        let content = t!("chat.nip17_not_found", u = profile.name());

                        this.insert_warning(content, cx);
                    }
                }
            }),
        );

        subscriptions.push(
            // Observe when user close chat panel
            cx.on_release_in(window, move |this, window, cx| {
                this.disconnect_relays(cx);
                this.messages.clear();
                this.rendered_texts_by_id.clear();
                this.reports_by_id.clear();
                this.image_cache.update(cx, |this, cx| {
                    this.clear(window, cx);
                });
            }),
        );

        Self {
            id: room.read(cx).id.to_string().into(),
            image_cache: RetainAllImageCache::new(cx),
            focus_handle: cx.focus_handle(),
            rendered_texts_by_id: BTreeMap::new(),
            reports_by_id: BTreeMap::new(),
            relays,
            messages,
            room,
            list_state,
            input,
            replies_to,
            attachments,
            uploading: false,
            _subscriptions: subscriptions,
            _tasks: tasks,
        }
    }

    /// Disconnect all relays when the user closes the chat panel
    fn disconnect_relays(&mut self, cx: &mut App) {
        let relays = self.relays.read(cx).clone();

        cx.background_spawn(async move {
            let client = nostr_client();

            for relay in relays.values().flatten() {
                client.disconnect_relay(relay).await.ok();
            }
        })
        .detach();
    }

    /// Load all messages belonging to this room
    fn load_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let load_messages = self.room.read(cx).load_messages(cx);

        self._tasks.push(
            // Run the task in the background
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
                            window.push_notification(e.to_string(), cx);
                        })
                        .ok();
                    }
                };
            }),
        );
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

    /// Check if the event is sent by Coop
    fn is_sent_by_coop(&self, gift_wrap_id: &EventId) -> bool {
        app_state().sent_ids.read_blocking().contains(gift_wrap_id)
    }

    /// Send a message to all members of the chat
    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Get the message which includes all attachments
        let content = self.input_content(cx);

        // Return if message is empty
        if content.trim().is_empty() {
            window.push_notification(t!("chat.empty_message_error"), cx);
            return;
        }

        // Temporary disable input
        self.input.update(cx, |this, cx| {
            this.set_loading(true, cx);
            this.set_disabled(true, cx);
        });

        // Get the backup setting
        let backup = AppSettings::get_backup_messages(cx);

        // Get replies_to if it's present
        let replies = self.replies_to.read(cx).clone();

        // Get the current room entity
        let room = self.room.read(cx);
        let identity = Registry::read_global(cx).identity(cx).public_key();

        // Create a temporary message for optimistic update
        let temp_message = room.create_temp_message(identity, &content, replies.as_ref());
        let temp_id = temp_message.id.unwrap();

        // Create a task for sending the message in the background
        let send_message = room.send_in_background(&content, replies, backup, cx);

        cx.defer_in(window, |this, window, cx| {
            // Optimistically update message list
            this.insert_message(Message::user(temp_message), true, cx);

            // Remove all replies
            this.remove_all_replies(cx);

            // remove all attachments
            this.remove_all_attachments(cx);

            // Reset the input state
            this.input.update(cx, |this, cx| {
                this.set_loading(false, cx);
                this.set_disabled(false, cx);
                this.set_value("", window, cx);
            });
        });

        // Continue sending the message in the background
        cx.spawn_in(window, async move |this, cx| {
            match send_message.await {
                Ok(reports) => {
                    this.update(cx, |this, cx| {
                        this.room.update(cx, |this, cx| {
                            if this.kind != RoomKind::Ongoing {
                                // Update the room kind to ongoing
                                // But keep the room kind if send failed
                                if reports.iter().all(|r| !r.is_sent_success()) {
                                    this.kind = RoomKind::Ongoing;
                                    cx.notify();
                                }
                            }
                        });

                        // Insert the sent reports
                        this.reports_by_id.insert(temp_id, reports);

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

    fn resend_message(&mut self, id: &EventId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(reports) = self.reports_by_id.get(id).cloned() {
            if let Some(message) = self.message(id) {
                let backup = AppSettings::get_backup_messages(cx);
                let id_clone = id.to_owned();
                let message = message.content.to_owned();
                let task = self.room.read(cx).resend(reports, message, backup, cx);

                cx.spawn_in(window, async move |this, cx| {
                    match task.await {
                        Ok(reports) => {
                            if !reports.is_empty() {
                                this.update(cx, |this, cx| {
                                    this.reports_by_id.entry(id_clone).and_modify(|this| {
                                        *this = reports;
                                    });
                                    cx.notify();
                                })
                                .ok();
                            }
                        }
                        Err(e) => {
                            cx.update(|window, cx| {
                                window.push_notification(e.to_string(), cx);
                            })
                            .ok();
                        }
                    };
                })
                .detach();
            }
        }
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
                cx.notify();
            }
        }
    }

    /// Convert and insert a vector of nostr events into the chat panel
    fn insert_messages(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        for event in events.into_iter() {
            let m = Message::user(event);
            self.insert_message(m, false, cx);
        }
        cx.notify();
    }

    fn insert_warning(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        let m = Message::warning(content.into());
        self.insert_message(m, true, cx);
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

    fn profile(&self, public_key: &PublicKey, cx: &Context<Self>) -> Profile {
        let registry = Registry::read_global(cx);
        registry.get_person(public_key, cx)
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
                let client = nostr_client();
                let file = fs::read(path).await.ok()?;
                let url = nip96_upload(client, &nip96_server, file).await.ok()?;

                Some(url)
            });

            if let Ok(task) = upload {
                this.update(cx, |this, cx| {
                    this.set_uploading(true, cx);
                })
                .ok();

                match Flatten::flatten(task.await.map_err(|e| e.into())) {
                    Ok(Some(url)) => {
                        this.update(cx, |this, cx| {
                            this.add_attachment(url, cx);
                            this.set_uploading(false, cx);
                        })
                        .ok();
                    }
                    Ok(None) => {
                        this.update_in(cx, |this, window, cx| {
                            window.push_notification("Failed to upload file", cx);
                            this.set_uploading(false, cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        this.update_in(cx, |this, window, cx| {
                            window.push_notification(Notification::error(e.to_string()), cx);
                            this.set_uploading(false, cx);
                        })
                        .ok();
                    }
                }
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

    fn render_announcement(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
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
            .into_any_element()
    }

    fn render_warning(&mut self, ix: usize, content: String, cx: &mut Context<Self>) -> AnyElement {
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
                    .child(SharedString::from(content)),
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

    fn render_message_not_found(&self, ix: usize, cx: &Context<Self>) -> AnyElement {
        div()
            .id(ix)
            .w_full()
            .py_1()
            .px_3()
            .child(
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(cx.theme().danger_foreground)
                    .child(SharedString::from(ix.to_string()))
                    .child(shared_t!("chat.not_found")),
            )
            .into_any_element()
    }

    fn render_message(
        &self,
        ix: usize,
        message: &RenderedMessage,
        text: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let proxy = AppSettings::get_proxy_user_avatars(cx);
        let hide_avatar = AppSettings::get_hide_user_avatars(cx);

        let id = message.id;
        let author = self.profile(&message.author, cx);

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
                        this.child(Avatar::new(author.avatar(proxy)).size(rems(2.)))
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
                                            .child(author.display_name()),
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
                                this.child(
                                    h_flex()
                                        .gap_1()
                                        .child(self.render_message_reports(&id, cx))
                                        .child(
                                            Button::new(SharedString::from(id.to_hex()))
                                                .label(t!("common.resend"))
                                                .danger()
                                                .xsmall()
                                                .rounded()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.resend_message(&id, window, cx);
                                                    },
                                                )),
                                        ),
                                )
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
                            .child(author.display_name()),
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
            .child(shared_t!("chat.sent"))
            .when_some(self.sent_reports(id).cloned(), |this, reports| {
                this.on_click(move |_e, window, cx| {
                    let reports = reports.clone();

                    window.open_modal(cx, move |this, _window, cx| {
                        this.show_close(true)
                            .title(shared_t!("chat.reports"))
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
            .child(shared_t!("chat.sent_failed"))
            .when_some(self.sent_reports(id).cloned(), |this, reports| {
                this.on_click(move |_e, window, cx| {
                    let reports = reports.clone();

                    window.open_modal(cx, move |this, _window, cx| {
                        this.show_close(true)
                            .title(shared_t!("chat.reports"))
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
        let registry = Registry::read_global(cx);
        let profile = registry.get_person(&report.receiver, cx);
        let name = profile.display_name();
        let avatar = profile.avatar(true);

        v_flex()
            .gap_2()
            .w_full()
            .child(
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(shared_t!("chat.sent_to"))
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
                                .child(shared_t!("chat.nip17_not_found", u = name)),
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
                                                .child(shared_t!("chat.sent_success")),
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
            .shadow_sm()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                Button::new("reply")
                    .icon(IconName::Reply)
                    .tooltip(t!("chat.reply_button"))
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
                    .tooltip(t!("chat.copy_message_button"))
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
                        move |this, _window, _cx| {
                            this.menu(t!("common.seen_on"), Box::new(SeenOn(id)))
                        }
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
                img(SharedUri::from(url.to_string()))
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

    fn subject_button(&self, cx: &App) -> Button {
        let room = self.room.downgrade();
        let subject = self
            .room
            .read(cx)
            .subject
            .as_ref()
            .map(|subject| subject.to_string());

        Button::new("subject")
            .icon(IconName::Edit)
            .tooltip(t!("chat.subject_tooltip"))
            .on_click(move |_, window, cx| {
                let view = subject::init(subject.clone(), window, cx);
                let room = room.clone();
                let weak_view = view.downgrade();

                window.open_modal(cx, move |this, _window, _cx| {
                    let room = room.clone();
                    let weak_view = weak_view.clone();

                    this.confirm()
                        .title(shared_t!("chat.subject_tooltip"))
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
            })
    }

    fn reload_button(&self, _cx: &App) -> Button {
        let room = self.room.downgrade();

        Button::new("reload")
            .icon(IconName::Refresh)
            .tooltip(t!("chat.reload_tooltip"))
            .on_click(move |_, window, cx| {
                window.push_notification(t!("common.refreshed"), cx);
                room.update(cx, |this, cx| {
                    this.emit_refresh(cx);
                })
                .ok();
            })
    }

    fn on_open_seen_on(&mut self, ev: &SeenOn, window: &mut Window, cx: &mut Context<Self>) {
        let id = ev.0;

        let task: Task<Result<Vec<RelayUrl>, Error>> = cx.background_spawn(async move {
            let client = nostr_client();
            let app_state = app_state();
            let mut relays: Vec<RelayUrl> = vec![];

            let filter = Filter::new()
                .kind(Kind::ApplicationSpecificData)
                .event(id)
                .limit(1);

            if let Some(event) = client.database().query(filter).await?.first_owned() {
                if let Some(Ok(id)) = event.tags.identifier().map(EventId::parse) {
                    if let Some(urls) = app_state.seen_on_relays.read().await.get(&id).cloned() {
                        relays.extend(urls);
                    }
                }
            }

            Ok(relays)
        });

        cx.spawn_in(window, async move |_, cx| {
            if let Ok(urls) = task.await {
                cx.update(|window, cx| {
                    window.open_modal(cx, move |this, _window, cx| {
                        this.title(shared_t!("common.seen_on")).child(
                            v_flex().pb_4().gap_2().children({
                                let mut items = Vec::with_capacity(urls.len());

                                for url in urls.clone().into_iter() {
                                    items.push(
                                        h_flex()
                                            .h_8()
                                            .px_2()
                                            .bg(cx.theme().elevated_surface_background)
                                            .rounded(cx.theme().radius)
                                            .font_semibold()
                                            .text_xs()
                                            .child(url.to_string()),
                                    )
                                }

                                items
                            }),
                        )
                    });
                })
                .ok();
            }
        })
        .detach();
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
        let subject_button = self.subject_button(cx);
        let reload_button = self.reload_button(cx);

        vec![subject_button, reload_button]
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
            .on_action(cx.listener(Self::on_open_seen_on))
            .image_cache(self.image_cache.clone())
            .size_full()
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(move |this, ix: usize, window, cx| {
                        if let Some(message) = this.messages.get_index(ix) {
                            match message {
                                Message::User(rendered) => {
                                    let text = this
                                        .rendered_texts_by_id
                                        .entry(rendered.id)
                                        .or_insert_with(|| RenderedText::new(&rendered.content, cx))
                                        .element(ix.into(), window, cx);

                                    this.render_message(ix, rendered, text, cx)
                                }
                                Message::Warning(content, _) => {
                                    this.render_warning(ix, content.to_owned(), cx)
                                }
                                Message::System(_) => this.render_announcement(ix, cx),
                            }
                        } else {
                            this.render_message_not_found(ix, cx)
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
                                        div()
                                            .flex()
                                            .items_center()
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
