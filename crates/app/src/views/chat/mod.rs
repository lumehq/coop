use async_utility::task::spawn;
use chat::room::Room;
use common::{
    constants::IMAGE_SERVICE,
    utils::{compare, message_time, nip96_upload},
};
use gpui::{
    div, img, list, px, white, AnyElement, App, AppContext, Context, Entity, EventEmitter, Flatten,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ListAlignment, ListState, ObjectFit,
    ParentElement, PathPromptOptions, Pixels, Render, SharedString, StatefulInteractiveElement,
    Styled, StyledImage, WeakEntity, Window,
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
    prelude::FluentBuilder,
    theme::{scale::ColorScaleStep, ActiveTheme},
    v_flex, ContextModal, Icon, IconName, Sizable,
};

mod message;

#[derive(Clone)]
pub struct State {
    count: usize,
    items: Vec<Message>,
}

pub struct ChatPanel {
    // Panel
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    id: SharedString,
    name: SharedString,
    room: Entity<Room>,
    state: Entity<State>,
    list: ListState,
    // New Message
    input: Entity<TextInput>,
    // Media
    attaches: Entity<Option<Vec<Url>>>,
    is_uploading: bool,
}

impl ChatPanel {
    pub fn new(model: Entity<Room>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let room = model.read(cx);
        let id = room.id.to_string().into();
        let name = room.title.clone().unwrap_or("Untitled".into());

        cx.new(|cx| {
            cx.observe_new::<Self>(|this, window, cx| {
                if let Some(window) = window {
                    this.load_messages(window, cx);
                }
            })
            .detach();

            // Form
            let input = cx.new(|cx| {
                TextInput::new(window, cx)
                    .appearance(false)
                    .text_size(ui::Size::Small)
                    .placeholder("Message...")
            });

            // List
            let state = cx.new(|_| State {
                count: 0,
                items: vec![],
            });

            // Send message when user presses enter
            cx.subscribe_in(
                &input,
                window,
                move |this: &mut ChatPanel, view, input_event, window, cx| {
                    if let InputEvent::PressEnter = input_event {
                        this.send_message(view.downgrade(), window, cx);
                    }
                },
            )
            .detach();

            // Update list on every state changes
            cx.observe(&state, |this, model, cx| {
                let items = model.read(cx).items.clone();

                this.list = ListState::new(
                    items.len(),
                    ListAlignment::Bottom,
                    Pixels(256.),
                    move |idx, _window, _cx| {
                        let item = items.get(idx).unwrap().clone();
                        div().child(item).into_any_element()
                    },
                );

                cx.notify();
            })
            .detach();

            cx.observe_in(&model, window, |this, model, window, cx| {
                this.load_new_messages(model.downgrade(), window, cx);
            })
            .detach();

            let attaches = cx.new(|_| None);

            Self {
                closeable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                room: model,
                list: ListState::new(0, ListAlignment::Bottom, Pixels(256.), move |_, _, _| {
                    div().into_any_element()
                }),
                is_uploading: false,
                id,
                name,
                input,
                state,
                attaches,
            }
        })
    }

    fn load_messages(&self, _window: &mut Window, cx: &mut Context<Self>) {
        let room = self.room.read(cx);
        let members = room.members.clone();
        let owner = room.owner.clone();
        // Get all public keys
        let all_keys = room.get_pubkeys();

        // Async
        let async_state = self.state.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let events: anyhow::Result<Events, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn({
                        let client = get_client();
                        let pubkeys = members.iter().map(|m| m.public_key()).collect::<Vec<_>>();

                        async move {
                            let signer = client.signer().await?;
                            let author = signer.get_public_key().await?;

                            let recv = Filter::new()
                                .kind(Kind::PrivateDirectMessage)
                                .author(author)
                                .pubkeys(pubkeys.clone());

                            let send = Filter::new()
                                .kind(Kind::PrivateDirectMessage)
                                .authors(pubkeys)
                                .pubkey(author);

                            // Get all DM events in database
                            let recv_events = client.database().query(recv).await?;
                            let send_events = client.database().query(send).await?;
                            let events = recv_events.merge(send_events);

                            Ok(events)
                        }
                    })
                    .await;

                if let Ok(events) = events {
                    let items: Vec<Message> = events
                        .into_iter()
                        .sorted_by_key(|ev| ev.created_at)
                        .filter_map(|ev| {
                            let mut pubkeys: Vec<_> = ev.tags.public_keys().copied().collect();
                            pubkeys.push(ev.pubkey);

                            if compare(&pubkeys, &all_keys) {
                                let member = if let Some(member) =
                                    members.iter().find(|&m| m.public_key() == ev.pubkey)
                                {
                                    member.to_owned()
                                } else {
                                    owner.clone()
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

                    let total = items.len();

                    _ = async_cx.update_entity(&async_state, |a, b| {
                        a.items = items;
                        a.count = total;
                        b.notify();
                    });
                }
            })
            .detach();
    }

    fn load_new_messages(
        &self,
        model: WeakEntity<Room>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(model) = model.upgrade() {
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

            cx.update_entity(&self.state, |model, cx| {
                let messages: Vec<Message> = items
                    .into_iter()
                    .filter_map(|new| {
                        if !model.items.iter().any(|old| old == &new) {
                            Some(new)
                        } else {
                            None
                        }
                    })
                    .collect();

                model.items.extend(messages);
                model.count = model.items.len();
                cx.notify();
            });
        }
    }

    fn send_message(
        &mut self,
        view: WeakEntity<TextInput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let window_handle = window.window_handle();
        let room = self.room.read(cx);
        let owner = room.owner.clone();
        let mut members = room.members.to_vec();
        members.push(owner.clone());

        // Get message
        let mut content = self.input.read(cx).text().to_string();

        if content.is_empty() {
            window.push_notification("Message cannot be empty", cx);
            return;
        }

        // Get all attaches and merge with message
        if let Some(attaches) = self.attaches.read(cx).as_ref() {
            let merged = attaches
                .iter()
                .map(|url| url.to_string())
                .collect::<Vec<_>>()
                .join("\n");

            content = format!("{}\n{}", content, merged)
        }

        // Update input state
        if let Some(input) = view.upgrade() {
            cx.update_entity(&input, |input, cx| {
                input.set_loading(true, window, cx);
                input.set_disabled(true, window, cx);
            });
        }

        cx.spawn(|this, mut cx| async move {
            // Send message to all members
            cx.background_executor()
                .spawn({
                    let client = get_client();
                    let content = content.clone().to_string();
                    let tags: Vec<Tag> = members
                        .iter()
                        .filter_map(|m| {
                            if m.public_key() != owner.public_key() {
                                Some(Tag::public_key(m.public_key()))
                            } else {
                                None
                            }
                        })
                        .collect();

                    async move {
                        // Send message to all members
                        for member in members.iter() {
                            _ = client
                                .send_private_msg(member.public_key(), &content, tags.clone())
                                .await
                        }
                    }
                })
                .detach();

            if let Some(view) = this.upgrade() {
                _ = cx.update_entity(&view, |this, cx| {
                    cx.update_entity(&this.state, |model, cx| {
                        let message = Message::new(
                            owner,
                            content.to_string().into(),
                            message_time(Timestamp::now()).into(),
                        );

                        model.items.extend(vec![message]);
                        model.count = model.items.len();
                        cx.notify();
                    });
                    cx.notify();
                });
            }

            if let Some(input) = view.upgrade() {
                cx.update_window(window_handle, |_, window, cx| {
                    cx.update_entity(&input, |input, cx| {
                        input.set_loading(false, window, cx);
                        input.set_disabled(false, window, cx);
                        input.set_text("", window, cx);
                    });
                })
                .unwrap()
            }
        })
        .detach();
    }

    fn upload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let attaches = self.attaches.clone();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        // Show loading spinner
        self.is_uploading = true;
        cx.notify();

        // TODO: support multiple upload
        cx.spawn(move |this, mut async_cx| async move {
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
                            // Stop loading spinner
                            if let Some(view) = this.upgrade() {
                                _ = async_cx.update_entity(&view, |this, cx| {
                                    this.is_uploading = false;
                                    cx.notify();
                                });
                            }

                            // Update attaches model
                            _ = async_cx.update_entity(&attaches, |model, cx| {
                                if let Some(model) = model.as_mut() {
                                    model.push(url);
                                } else {
                                    *model = Some(vec![url]);
                                }
                                cx.notify();
                            });
                        }
                    }
                }
                Ok(None) => {}
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
}

impl Panel for ChatPanel {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn panel_facepile(&self, cx: &App) -> Option<Vec<String>> {
        Some(
            self.room
                .read(cx)
                .members
                .iter()
                .map(|member| member.avatar())
                .collect(),
        )
    }

    fn title(&self, _cx: &App) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _cx: &App) -> bool {
        self.closeable
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

impl EventEmitter<PanelEvent> for ChatPanel {}

impl Focusable for ChatPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatPanel {
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
                                                    this.send_message(
                                                        this.input.downgrade(),
                                                        window,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                ),
                        ),
                ),
            )
    }
}
