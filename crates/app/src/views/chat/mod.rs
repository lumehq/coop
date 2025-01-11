use crate::{get_client, states::chat::room::Room};
use gpui::{
    div, list, px, AnyElement, AppContext, Context, EventEmitter, Flatten, FocusHandle,
    FocusableView, IntoElement, ListAlignment, ListState, Model, ParentElement, PathPromptOptions,
    Pixels, Render, SharedString, Styled, View, ViewContext, VisualContext, WindowContext,
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
    name: SharedString,
    closeable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
    // Chat Room
    id: SharedString,
    room: Arc<Room>,
    input: View<TextInput>,
    list: ListState,
    state: Model<State>,
}

impl ChatPanel {
    pub fn new(room_id: &u64, cx: &mut WindowContext) -> View<Self> {
        let room = Arc::new(room);
        let id = room.id.clone();
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

            // Send message when user presses enter on form.
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

            let list = ListState::new(0, ListAlignment::Bottom, Pixels(256.), move |_, _| {
                div().into_any_element()
            });

            Self {
                closeable: true,
                zoomable: true,
                focus_handle: cx.focus_handle(),
                id,
                name,
                room,
                input,
                list,
                state,
            }
        })
    }

    fn load_messages(&self, cx: &mut ViewContext<Self>) {
        let members = self.room.members.clone();
        let async_state = self.state.clone();
        let id = self.room.id.to_string();

        let client = get_client();
        let mut async_cx = cx.to_async();

        cx.foreground_executor()
            .spawn(async move {
                let events: anyhow::Result<Events, anyhow::Error> = async_cx
                    .background_executor()
                    .spawn({
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
                        .map(|ev| {
                            let metadata = members
                                .iter()
                                .find(|&m| m.public_key() == ev.pubkey)
                                .unwrap()
                                .metadata();

                            RoomMessage::new(ev.pubkey, metadata, ev.content, ev.created_at)
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

    fn send_message(&mut self, cx: &mut ViewContext<Self>) {}
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
