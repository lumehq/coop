use async_utility::task::spawn;
use chat_state::room::Room;
use common::{
    constants::IMAGE_SERVICE,
    profile::NostrProfile,
    utils::{compare, message_time, nip96_upload},
};
use gpui::{
    div, img, list, prelude::FluentBuilder, px, white, AnyElement, App, AppContext, Context,
    Entity, EventEmitter, Flatten, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ListAlignment, ListState, ObjectFit, ParentElement, PathPromptOptions, Pixels, Render,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, Window,
};
use itertools::Itertools;
use message::Message;
use nostr_sdk::prelude::*;
use smol::fs;
use state::get_client;
use tokio::sync::oneshot;
use ui::{
    button::{Button, ButtonRounded, ButtonVariants},
    dock_area::panel::{Panel, PanelEvent},
    input::{InputEvent, TextInput},
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, ContextModal, Icon, IconName, Sizable,
};

mod message;

pub fn init(room: &Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Chat> {
    Chat::new(room, window, cx)
}

#[derive(Clone)]
pub struct State {
    count: usize,
    items: Vec<Message>,
}

pub struct Chat {
    // Panel
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    id: SharedString,
    name: SharedString,
    owner: NostrProfile,
    members: Vec<NostrProfile>,
    state: Entity<State>,
    list: ListState,
    // New Message
    input: Entity<TextInput>,
    // Media
    attaches: Entity<Option<Vec<Url>>>,
    is_uploading: bool,
}

impl Chat {
    pub fn new(model: &Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let room = model.read(cx);
        let id = room.id.to_string().into();
        let name = room.title.clone().unwrap_or("Untitled".into());
        let owner = room.owner.clone();
        let members = room.members.clone();

        cx.new(|cx| {
            // Load all messages
            cx.observe_new::<Self>(|this, window, cx| {
                if let Some(window) = window {
                    this.load_messages(window, cx);
                }
            })
            .detach();

            // Observe and load new messages
            cx.observe_in(model, window, |this: &mut Chat, model, _, cx| {
                this.load_new_messages(&model, cx);
            })
            .detach();

            // New message form
            let input = cx.new(|cx| {
                TextInput::new(window, cx)
                    .appearance(false)
                    .text_size(ui::Size::Small)
                    .placeholder("Message...")
            });

            // Send message when user presses enter
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut Chat, _, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.send_message(window, cx);
                    }
                },
            )
            .detach();

            // List state model
            let state = cx.new(|_| State {
                count: 0,
                items: vec![],
            });

            // Update list on every state changes
            cx.observe(&state, |this, model, cx| {
                this.list = ListState::new(
                    model.read(cx).items.len(),
                    ListAlignment::Bottom,
                    Pixels(1024.),
                    move |idx, _window, cx| {
                        if let Some(message) = model.read(cx).items.get(idx) {
                            div().child(message.clone()).into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    },
                );
                cx.notify();
            })
            .detach();

            let attaches = cx.new(|_| None);

            Self {
                closable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                list: ListState::new(0, ListAlignment::Bottom, Pixels(1024.), move |_, _, _| {
                    div().into_any_element()
                }),
                is_uploading: false,
                id,
                name,
                owner,
                members,
                input,
                state,
                attaches,
            }
        })
    }

    fn load_messages(&self, window: &mut Window, cx: &mut Context<Self>) {
        let window_handle = window.window_handle();
        // Get current user
        let author = self.owner.public_key();
        // Get other users in room
        let pubkeys = self
            .members
            .iter()
            .map(|m| m.public_key())
            .collect::<Vec<_>>();
        // Get all public keys for comparisation
        let mut all_keys = pubkeys.clone();
        all_keys.push(author);

        cx.spawn(|this, mut cx| async move {
            let (tx, rx) = oneshot::channel::<Events>();

            cx.background_spawn({
                let client = get_client();

                let recv = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .author(author)
                    .pubkeys(pubkeys.iter().copied());

                let send = Filter::new()
                    .kind(Kind::PrivateDirectMessage)
                    .authors(pubkeys)
                    .pubkey(author);

                // Get all DM events in database
                async move {
                    let recv_events = client.database().query(recv).await.unwrap();
                    let send_events = client.database().query(send).await.unwrap();
                    let events = recv_events.merge(send_events);
                    _ = tx.send(events);
                }
            })
            .detach();

            if let Ok(events) = rx.await {
                _ = cx.update_window(window_handle, |_, _, cx| {
                    _ = this.update(cx, |this, cx| {
                        let items: Vec<Message> = events
                            .into_iter()
                            .sorted_by_key(|ev| ev.created_at)
                            .filter_map(|ev| {
                                let mut pubkeys: Vec<_> = ev.tags.public_keys().copied().collect();
                                pubkeys.push(ev.pubkey);

                                if compare(&pubkeys, &all_keys) {
                                    let member = if let Some(member) =
                                        this.members.iter().find(|&m| m.public_key() == ev.pubkey)
                                    {
                                        member.to_owned()
                                    } else {
                                        this.owner.clone()
                                    };

                                    Some(Message::new(
                                        member,
                                        ev.content.into(),
                                        message_time(ev.created_at).into(),
                                    ))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        cx.update_entity(&this.state, |this, cx| {
                            this.count = items.len();
                            this.items = items;
                            cx.notify();
                        });
                    });
                });
            }
        })
        .detach();
    }

    fn load_new_messages(&self, model: &Entity<Room>, cx: &mut Context<Self>) {
        let room = model.read(cx);
        let items: Vec<Message> = room
            .new_messages
            .iter()
            .filter_map(|event| {
                room.member(&event.pubkey).map(|member| {
                    Message::new(
                        member,
                        event.content.clone().into(),
                        message_time(event.created_at).into(),
                    )
                })
            })
            .collect();

        cx.update_entity(&self.state, |this, cx| {
            let messages: Vec<Message> = items
                .into_iter()
                .filter_map(|new| {
                    if !this.items.iter().any(|old| old == &new) {
                        Some(new)
                    } else {
                        None
                    }
                })
                .collect();

            this.items.extend(messages);
            this.count = this.items.len();
            cx.notify();
        });
    }

    fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_handle = window.window_handle();

        // Get current user
        let author = self.owner.public_key();

        // Get other users in room
        let mut pubkeys = self
            .members
            .iter()
            .map(|m| m.public_key())
            .collect::<Vec<_>>();
        pubkeys.push(author);

        // Get message
        let mut content = self.input.read(cx).text().to_string();

        // Get all attaches and merge with message
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

        cx.spawn(|this, mut cx| async move {
            cx.background_spawn({
                let client = get_client();
                let content = content.clone();
                let tags: Vec<Tag> = pubkeys
                    .iter()
                    .filter_map(|pubkey| {
                        if pubkey != &author {
                            Some(Tag::public_key(*pubkey))
                        } else {
                            None
                        }
                    })
                    .collect();

                async move {
                    // Send message to all members
                    for pubkey in pubkeys.iter() {
                        if let Err(_e) = client
                            .send_private_msg(*pubkey, &content, tags.clone())
                            .await
                        {
                            // TODO: handle error
                        }
                    }
                }
            })
            .detach();

            _ = cx.update_window(window_handle, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    let message = Message::new(
                        this.owner.clone(),
                        content.to_string().into(),
                        message_time(Timestamp::now()).into(),
                    );

                    // Update message list
                    cx.update_entity(&this.state, |this, cx| {
                        this.items.extend(vec![message]);
                        this.count = this.items.len();
                        cx.notify();
                    });

                    // Reset message input
                    cx.update_entity(&this.input, |this, cx| {
                        this.set_loading(false, window, cx);
                        this.set_disabled(false, window, cx);
                        this.set_text("", window, cx);
                        cx.notify();
                    });
                });
            });
        })
        .detach();
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
}

impl Panel for Chat {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn panel_facepile(&self, _cx: &App) -> Option<Vec<String>> {
        Some(self.members.iter().map(|member| member.avatar()).collect())
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
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
            .child(list(self.list.clone()).flex_1())
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
