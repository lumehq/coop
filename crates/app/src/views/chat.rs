use anyhow::anyhow;
use async_utility::task::spawn;
use chats::{registry::ChatRegistry, room::Room};
use common::{
    constants::IMAGE_SERVICE,
    last_seen::LastSeen,
    profile::NostrProfile,
    utils::{compare, nip96_upload},
};
use gpui::{
    div, img, list, prelude::FluentBuilder, px, white, AnyElement, App, AppContext, Context,
    Element, Entity, EventEmitter, Flatten, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ListAlignment, ListState, ObjectFit, ParentElement, PathPromptOptions, Render,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, Subscription, WeakEntity,
    Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use smol::fs;
use state::get_client;
use std::sync::Arc;
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    input::{InputEvent, TextInput},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, ContextModal, Icon, IconName, Sizable, StyledExt,
};

pub fn init(
    id: &u64,
    window: &mut Window,
    cx: &mut App,
) -> Result<Arc<Entity<Chat>>, anyhow::Error> {
    if let Some(chats) = ChatRegistry::global(cx) {
        if let Some(room) = chats.read(cx).get(id, cx) {
            Ok(Arc::new(Chat::new(id, &room, window, cx)))
        } else {
            Err(anyhow!("Chat room is not exist"))
        }
    } else {
        Err(anyhow!("Chat Registry is not initialized"))
    }
}

struct Message {
    profile: NostrProfile,
    content: SharedString,
    ago: SharedString,
}

impl PartialEq for Message {
    fn eq(&self, other: &Self) -> bool {
        let content = self.content == other.content;
        let member = self.profile == other.profile;
        let ago = self.ago == other.ago;

        content && member && ago
    }
}

impl Message {
    pub fn new(profile: NostrProfile, content: SharedString, ago: SharedString) -> Self {
        Self {
            profile,
            content,
            ago,
        }
    }
}
pub struct Chat {
    // Panel
    id: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    room: WeakEntity<Room>,
    messages: Entity<Vec<Message>>,
    new_messages: WeakEntity<Vec<Event>>,
    list_state: ListState,
    subscriptions: Vec<Subscription>,
    // New Message
    input: Entity<TextInput>,
    // Media
    attaches: Entity<Option<Vec<Url>>>,
    is_uploading: bool,
}

impl Chat {
    pub fn new(id: &u64, model: &Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let room = model.downgrade();
        let new_messages = model.read(cx).new_messages.downgrade();

        cx.new(|cx| {
            let messages = cx.new(|_| Vec::new());
            let attaches = cx.new(|_| None);

            let input = cx.new(|cx| {
                TextInput::new(window, cx)
                    .appearance(false)
                    .text_size(ui::Size::Small)
                    .placeholder("Message...")
            });

            let subscriptions = vec![cx.subscribe_in(
                &input,
                window,
                move |this: &mut Chat, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.send_message(window, cx);
                    }
                },
            )];

            let list_state = ListState::new(0, ListAlignment::Bottom, px(1024.), {
                let this = cx.entity().downgrade();
                move |ix, window, cx| {
                    this.update(cx, |this, cx| {
                        this.render_message(ix, window, cx).into_any_element()
                    })
                    .unwrap()
                }
            });

            let mut this = Self {
                closable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                is_uploading: false,
                id: id.to_string().into(),
                room,
                new_messages,
                messages,
                list_state,
                input,
                attaches,
                subscriptions,
            };

            // Load all messages from database
            this.load_messages(cx);
            // Subscribe and load new messages
            this.load_new_messages(cx);

            this
        })
    }

    fn load_messages(&self, cx: &mut Context<Self>) {
        let Some(model) = self.room.upgrade() else {
            return;
        };

        let client = get_client();
        let (tx, rx) = oneshot::channel::<Events>();

        let room = model.read(cx);
        let pubkeys = room
            .members
            .iter()
            .map(|m| m.public_key())
            .collect::<Vec<_>>();

        let recv = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .author(room.owner.public_key())
            .pubkeys(pubkeys.iter().copied());

        let send = Filter::new()
            .kind(Kind::PrivateDirectMessage)
            .authors(pubkeys)
            .pubkey(room.owner.public_key());

        cx.background_spawn(async move {
            let Ok(recv_events) = client.database().query(recv).await else {
                return;
            };
            let Ok(send_events) = client.database().query(send).await else {
                return;
            };
            let events = recv_events.merge(send_events);

            _ = tx.send(events);
        })
        .detach();

        cx.spawn(|this, cx| async move {
            if let Ok(events) = rx.await {
                _ = cx.update(|cx| {
                    _ = this.update(cx, |this, cx| {
                        this.push_messages(events, cx);
                    });
                })
            }
        })
        .detach();
    }

    fn push_messages(&self, events: Events, cx: &mut Context<Self>) {
        let Some(model) = self.room.upgrade() else {
            return;
        };

        let room = model.read(cx);
        let pubkeys = room.pubkeys();

        let (messages, total) = {
            let items: Vec<Message> = events
                .into_iter()
                .sorted_by_key(|ev| ev.created_at)
                .filter_map(|ev| {
                    let mut other_pubkeys: Vec<_> = ev.tags.public_keys().copied().collect();
                    other_pubkeys.push(ev.pubkey);

                    if compare(&other_pubkeys, &pubkeys) {
                        let member = if let Some(member) =
                            room.members.iter().find(|&m| m.public_key() == ev.pubkey)
                        {
                            member.to_owned()
                        } else {
                            room.owner.to_owned()
                        };

                        Some(Message::new(
                            member,
                            ev.content.into(),
                            LastSeen(ev.created_at).human_readable(),
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            let total = items.len();

            (items, total)
        };

        cx.update_entity(&self.messages, |this, cx| {
            this.extend(messages);
            cx.notify();
        });

        self.list_state.reset(total);
    }

    fn load_new_messages(&mut self, cx: &mut Context<Self>) {
        let Some(model) = self.new_messages.upgrade() else {
            return;
        };

        let subscription = cx.observe(&model, |view, this, cx| {
            let Some(model) = view.room.upgrade() else {
                return;
            };

            let room = model.read(cx);
            let old_messages = view.messages.read(cx);
            let old_len = old_messages.len();

            let items: Vec<Message> = this
                .read(cx)
                .iter()
                .filter_map(|event| {
                    if let Some(profile) = room.member(&event.pubkey) {
                        let message = Message::new(
                            profile,
                            event.content.clone().into(),
                            LastSeen(event.created_at).human_readable(),
                        );

                        if !old_messages.iter().any(|old| old == &message) {
                            Some(message)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            let total = items.len();

            cx.update_entity(&view.messages, |this, cx| {
                let messages: Vec<Message> = items
                    .into_iter()
                    .filter_map(|new| {
                        if !this.iter().any(|old| old == &new) {
                            Some(new)
                        } else {
                            None
                        }
                    })
                    .collect();

                this.extend(messages);
                cx.notify();
            });

            view.list_state.splice(old_len..old_len, total);
        });

        self.subscriptions.push(subscription);
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = self.room.upgrade() else {
            return;
        };

        // Get message
        let mut content = self.input.read(cx).text().to_string();

        // Get all attaches and merge its with message
        if let Some(attaches) = self.attaches.read(cx).as_ref() {
            let merged = attaches
                .iter()
                .map(|url| url.to_string())
                .collect::<Vec<_>>()
                .join("\n");

            content = format!("{}\n{}", content, merged)
        }

        if content.is_empty() {
            window.push_notification("Cannot send an empty message", cx);
            return;
        }

        // Disable input when sending message
        self.input.update(cx, |this, cx| {
            this.set_loading(true, window, cx);
            this.set_disabled(true, window, cx);
        });

        let client = get_client();
        let window_handle = window.window_handle();

        let room = model.read(cx);
        let pubkeys = room.pubkeys();
        let async_content = content.clone();
        let tags: Vec<Tag> = room
            .pubkeys()
            .iter()
            .filter_map(|pubkey| {
                if pubkey != &room.owner.public_key() {
                    Some(Tag::public_key(*pubkey))
                } else {
                    None
                }
            })
            .collect();

        // Send message to all pubkeys
        cx.background_spawn(async move {
            for pubkey in pubkeys.iter() {
                if let Err(_e) = client
                    .send_private_msg(*pubkey, &async_content, tags.clone())
                    .await
                {
                    // TODO: handle error
                }
            }
        })
        .detach();

        cx.spawn(|this, mut cx| async move {
            _ = cx.update_window(window_handle, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.force_push_message(content.clone(), window, cx);
                });
            });
        })
        .detach();
    }

    fn force_push_message(&self, content: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = self.room.upgrade() else {
            return;
        };

        let room = model.read(cx);
        let ago = LastSeen(Timestamp::now()).human_readable();
        let message = Message::new(room.owner.clone(), content.into(), ago);
        let old_len = self.messages.read(cx).len();

        // Update message list
        cx.update_entity(&self.messages, |this, cx| {
            this.extend(vec![message]);
            cx.notify();
        });

        // Reset message input
        cx.update_entity(&self.input, |this, cx| {
            this.set_loading(false, window, cx);
            this.set_disabled(false, window, cx);
            this.set_text("", window, cx);
            cx.notify();
        });

        self.list_state.splice(old_len..old_len, 1);
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_handle = window.window_handle();

        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        // Show loading spinner
        self.set_loading(true, cx);

        // TODO: support multiple upload
        cx.spawn(move |this, mut cx| async move {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    let path = paths.pop().unwrap();

                    if let Ok(file_data) = fs::read(path).await {
                        let (tx, rx) = oneshot::channel::<Url>();

                        spawn(async move {
                            let client = get_client();
                            if let Ok(url) = nip96_upload(client, file_data).await {
                                _ = tx.send(url);
                            }
                        });

                        if let Ok(url) = rx.await {
                            _ = cx.update_window(window_handle, |_, _, cx| {
                                _ = this.update(cx, |this, cx| {
                                    // Stop loading spinner
                                    this.set_loading(false, cx);

                                    this.attaches.update(cx, |this, cx| {
                                        if let Some(model) = this.as_mut() {
                                            model.push(url);
                                        } else {
                                            *this = Some(vec![url]);
                                        }
                                        cx.notify();
                                    });
                                });
                            });
                        }
                    }
                }
                Ok(None) => {
                    // Stop loading spinner
                    if let Some(view) = this.upgrade() {
                        cx.update_entity(&view, |this, cx| {
                            this.set_loading(false, cx);
                        })
                        .unwrap();
                    }
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    fn remove(&mut self, url: &Url, _window: &mut Window, cx: &mut Context<Self>) {
        self.attaches.update(cx, |model, cx| {
            if let Some(urls) = model.as_mut() {
                let ix = urls.iter().position(|x| x == url).unwrap();
                urls.remove(ix);
                cx.notify();
            }
        });
    }

    fn set_loading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_uploading = status;
        cx.notify();
    }

    fn render_message(
        &self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if let Some(message) = self.messages.read(cx).get(ix) {
            div()
                .group("")
                .relative()
                .flex()
                .gap_3()
                .w_full()
                .p_2()
                .hover(|this| this.bg(cx.theme().accent.step(cx, ColorScaleStep::ONE)))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(cx.theme().transparent)
                        .group_hover("", |this| {
                            this.bg(cx.theme().accent.step(cx, ColorScaleStep::NINE))
                        }),
                )
                .child(
                    img(message.profile.avatar())
                        .size_8()
                        .rounded_full()
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_initial()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .text_xs()
                                .child(div().font_semibold().child(message.profile.name()))
                                .child(
                                    div().child(message.ago.clone()).text_color(
                                        cx.theme().base.step(cx, ColorScaleStep::ELEVEN),
                                    ),
                                ),
                        )
                        .child(div().text_sm().child(message.content.clone())),
                )
        } else {
            div()
        }
    }
}

impl Panel for Chat {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn title(&self, cx: &App) -> AnyElement {
        self.room
            .read_with(cx, |this, _cx| {
                let name = this.name();
                let facepill: Vec<String> =
                    this.members.iter().map(|member| member.avatar()).collect();

                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row_reverse()
                            .items_center()
                            .justify_start()
                            .children(facepill.into_iter().enumerate().rev().map(|(ix, face)| {
                                div().when(ix > 0, |div| div.ml_neg_1()).child(
                                    img(face)
                                        .size_4()
                                        .rounded_full()
                                        .object_fit(ObjectFit::Cover),
                                )
                            })),
                    )
                    .child(name)
                    .into_any()
            })
            .unwrap_or("Unnamed".into_any())
    }

    fn closable(&self, _cx: &App) -> bool {
        self.closable
    }

    fn zoomable(&self, _cx: &App) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &App) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _window: &Window, _cx: &App) -> Vec<Button> {
        vec![]
    }
}

impl EventEmitter<PanelEvent> for Chat {}

impl Focusable for Chat {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Chat {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(list(self.list_state.clone()).flex_1())
            .child(
                div().flex_shrink_0().p_2().child(
                    div()
                        .flex()
                        .flex_col()
                        .when_some(self.attaches.read(cx).as_ref(), |this, attaches| {
                            this.gap_1p5().children(attaches.iter().map(|url| {
                                let url = url.clone();
                                let path: SharedString = url.to_string().into();

                                div()
                                    .id(path.clone())
                                    .relative()
                                    .w_16()
                                    .child(
                                        img(format!(
                                            "{}/?url={}&w=128&h=128&fit=cover&n=-1",
                                            IMAGE_SERVICE, path
                                        ))
                                        .size_16()
                                        .shadow_lg()
                                        .rounded(px(cx.theme().radius))
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
                                            .bg(cx.theme().danger)
                                            .child(
                                                Icon::new(IconName::Close)
                                                    .size_2()
                                                    .text_color(white()),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.remove(&url, window, cx);
                                    }))
                            }))
                        })
                        .child(
                            div()
                                .w_full()
                                .h_9()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("upload")
                                        .icon(Icon::new(IconName::Upload))
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.upload(window, cx);
                                        }))
                                        .loading(self.is_uploading),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                                        .rounded(px(cx.theme().radius))
                                        .pl_2()
                                        .pr_1()
                                        .child(self.input.clone())
                                        .child(
                                            Button::new("send")
                                                .ghost()
                                                .xsmall()
                                                .bold()
                                                .rounded(ButtonRounded::Medium)
                                                .label("SEND")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.send_message(window, cx)
                                                })),
                                        ),
                                ),
                        ),
                ),
            )
    }
}
