use crate::{get_client, states::chat::room::Room, utils::compare};
use gpui::{
    div, list, px, AnyElement, AppContext, Context, EventEmitter, Flatten, FocusHandle,
    FocusableView, IntoElement, ListAlignment, ListState, Model, ParentElement, PathPromptOptions,
    Pixels, Render, SharedString, Styled, View, ViewContext, VisualContext, WeakModel,
    WindowContext,
};
use itertools::Itertools;
use message::RoomMessage;
use nostr_sdk::prelude::*;
use std::sync::Arc;
use ui::{
    button::{Button, ButtonVariants},
    dock::{Panel, PanelEvent, PanelState},
    input::{InputEvent, TextInput},
    popup_menu::PopupMenu,
    theme::ActiveTheme,
    v_flex, Icon, IconName,
};

mod message;

#[derive(Clone)]
pub struct State {
    count: usize,
    items: Vec<RoomMessage>,
}

pub struct ChatPanel {
    // Panel
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    id: SharedString,
    name: SharedString,
    room: Model<Room>,
    state: Model<State>,
    list: ListState,
    input: View<TextInput>,
}

impl ChatPanel {
    pub fn new(model: Model<Room>, cx: &mut WindowContext) -> View<Self> {
        let room = model.read(cx);
        let id = room.id.to_string().into();
        let name = room.title.clone().unwrap_or("Untitled".into());

        cx.observe_new_views::<Self>(|this, cx| {
            this.load_messages(cx);
        })
        .detach();

        cx.new_view(|cx| {
            // Form
            let input = cx.new_view(|cx| {
                TextInput::new(cx)
                    .appearance(false)
                    .text_size(ui::Size::Small)
                    .placeholder("Message...")
                    .cleanable()
            });

            // Send message when user presses enter
            cx.subscribe(&input, move |this: &mut ChatPanel, _, input_event, cx| {
                if let InputEvent::PressEnter = input_event {
                    this.send_message(cx);
                }
            })
            .detach();

            let state = cx.new_model(|_| State {
                count: 0,
                items: vec![],
            });

            // Update list on every state changes
            cx.observe(&state, |this, model, cx| {
                let items = model.read(cx).items.clone();

                this.list = ListState::new(
                    items.len(),
                    ListAlignment::Bottom,
                    Pixels(256.),
                    move |idx, _cx| {
                        let item = items.get(idx).unwrap().clone();
                        div().child(item).into_any_element()
                    },
                );

                cx.notify();
            })
            .detach();

            cx.observe(&model, |this, model, cx| {
                this.load_new_messages(model.downgrade(), cx);
            })
            .detach();

            Self {
                closeable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                room: model,
                list: ListState::new(0, ListAlignment::Bottom, Pixels(256.), move |_, _| {
                    div().into_any_element()
                }),
                id,
                name,
                input,
                state,
            }
        })
    }

    fn load_messages(&self, cx: &mut ViewContext<Self>) {
        let room = self.room.read(cx);
        let members = room.members.clone();
        let owner = room.owner.clone();

        // Get all public keys
        let mut all_keys: Vec<_> = room.members.iter().map(|m| m.public_key()).collect();
        all_keys.push(room.owner.public_key());

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
                            let query = client.database().query(vec![recv, send]).await?;

                            Ok(query)
                        }
                    })
                    .await;

                if let Ok(events) = events {
                    let items: Vec<RoomMessage> = events
                        .into_iter()
                        .sorted_by_key(|ev| ev.created_at)
                        .filter_map(|ev| {
                            let mut pubkeys: Vec<_> = ev.tags.public_keys().copied().collect();
                            pubkeys.push(ev.pubkey);

                            if compare(&pubkeys, &all_keys) {
                                let metadata = if let Some(member) =
                                    members.iter().find(|&m| m.public_key() == ev.pubkey)
                                {
                                    member.metadata()
                                } else if ev.pubkey == owner.public_key() {
                                    owner.metadata()
                                } else {
                                    Metadata::default()
                                };

                                Some(RoomMessage::new(
                                    ev.pubkey,
                                    metadata,
                                    ev.content,
                                    ev.created_at,
                                ))
                            } else {
                                None
                            }
                        })
                        .collect();

                    let total = items.len();

                    _ = async_cx.update_model(&async_state, |a, b| {
                        a.items = items;
                        a.count = total;
                        b.notify();
                    });
                }
            })
            .detach();
    }

    fn load_new_messages(&self, model: WeakModel<Room>, cx: &mut ViewContext<Self>) {
        if let Some(model) = model.upgrade() {
            let room = model.read(cx);
            let items: Vec<RoomMessage> = room
                .new_messages
                .iter()
                .map(|event| {
                    let metadata = room.metadata(event.pubkey);

                    RoomMessage::new(
                        event.pubkey,
                        metadata,
                        event.content.clone(),
                        event.created_at,
                    )
                })
                .collect();

            cx.update_model(&self.state, |model, cx| {
                model.items.extend(items);
                model.count = model.items.len();
                cx.notify();
            });
        }
    }

    fn send_message(&mut self, cx: &mut ViewContext<Self>) {
        let room = self.room.read(cx);
        let content = Arc::new(self.input.read(cx).text().to_string());
        let owner = room.owner.clone();

        let mut members = room.members.to_vec();
        members.push(owner.clone());

        // Async
        let async_input = self.input.clone();
        let async_state = self.state.clone();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                // Send message to all members
                async_cx
                    .background_executor()
                    .spawn({
                        let client = get_client();
                        let content = Arc::clone(&content).to_string();
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

                _ = async_cx.update_model(&async_state, |model, cx| {
                    let created_at = Timestamp::now();
                    let message = RoomMessage::new(
                        owner.public_key(),
                        owner.metadata(),
                        content.to_string(),
                        created_at,
                    );

                    model.items.extend(vec![message]);
                    model.count = model.items.len();
                    cx.notify();
                });

                _ = async_cx.update_view(&async_input, |input, cx| {
                    input.set_text("", cx);
                });
            })
            .detach();
    }
}

impl Panel for ChatPanel {
    fn panel_id(&self) -> SharedString {
        self.id.clone()
    }

    fn panel_metadata(&self) -> Option<Metadata> {
        None
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn closeable(&self, _cx: &WindowContext) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &WindowContext) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
    }

    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &AppContext) -> PanelState {
        PanelState::new(self)
    }
}

impl EventEmitter<PanelEvent> for ChatPanel {}

impl FocusableView for ChatPanel {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatPanel {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(list(self.list.clone()).flex_1())
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .h_12()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .child(
                        Button::new("upload")
                            .icon(Icon::new(IconName::Upload))
                            .ghost()
                            .on_click(|_, cx| {
                                let paths = cx.prompt_for_paths(PathPromptOptions {
                                    files: true,
                                    directories: false,
                                    multiple: false,
                                });

                                cx.spawn(move |_async_cx| async move {
                                    match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                                        Ok(Some(paths)) => {
                                            // TODO: upload file to blossom server
                                            println!("Paths: {:?}", paths)
                                        }
                                        Ok(None) => {}
                                        Err(_) => {}
                                    }
                                })
                                .detach();
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .bg(cx.theme().muted)
                            .rounded(px(cx.theme().radius))
                            .px_2()
                            .child(self.input.clone()),
                    ),
            )
    }
}
