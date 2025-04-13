use std::collections::BTreeSet;

use anyhow::Error;
use common::profile::SharedProfile;
use global::get_client;
use gpui::{
    div, img, prelude::FluentBuilder, px, uniform_list, AnyElement, App, AppContext, Context,
    Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, Styled, Task, Window,
};
use itertools::Itertools;
use nostr_sdk::prelude::*;
use ui::{
    button::Button,
    dock_area::panel::{Panel, PanelEvent},
    indicator::Indicator,
    popup_menu::PopupMenu,
    theme::{scale::ColorScaleStep, ActiveTheme},
    Sizable,
};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<Contacts> {
    Contacts::new(window, cx)
}

pub struct Contacts {
    contacts: Option<Vec<Profile>>,
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl Contacts {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            let client = get_client();

            let task: Task<Result<BTreeSet<Profile>, Error>> = cx.background_spawn(async move {
                let signer = client.signer().await?;
                let public_key = signer.get_public_key().await?;
                let profiles = client.database().contacts(public_key).await?;

                Ok(profiles)
            });

            if let Ok(contacts) = task.await {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.contacts = Some(contacts.into_iter().collect_vec());
                        cx.notify();
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();

        Self {
            contacts: None,
            name: "Contacts".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Panel for Contacts {
    fn panel_id(&self) -> SharedString {
        "ContactPanel".into()
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

impl EventEmitter<PanelEvent> for Contacts {}

impl Focusable for Contacts {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Contacts {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().pt_2().px_2().map(|this| {
            if let Some(contacts) = self.contacts.clone() {
                this.child(
                    uniform_list(
                        cx.entity().clone(),
                        "contacts",
                        contacts.len(),
                        move |_, range, _window, cx| {
                            let mut items = Vec::new();

                            for ix in range {
                                let item = contacts.get(ix).unwrap().clone();

                                items.push(
                                    div()
                                        .w_full()
                                        .h_9()
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .rounded(px(cx.theme().radius))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .text_xs()
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .child(img(item.shared_avatar()).size_6()),
                                                )
                                                .child(item.shared_name()),
                                        )
                                        .hover(|this| {
                                            this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE))
                                        }),
                                );
                            }

                            items
                        },
                    )
                    .h_full(),
                )
            } else {
                this.flex()
                    .items_center()
                    .justify_center()
                    .h_16()
                    .child(Indicator::new().small())
            }
        })
    }
}
