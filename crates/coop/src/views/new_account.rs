use std::str::FromStr;

use app_state::AppState;
use async_utility::task::spawn;
use common::nip96_upload;
use global::shared_state;
use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, relative, AnyElement, App, AppContext, Context, Entity, EventEmitter, Flatten, FocusHandle, Focusable,
    IntoElement, ParentElement, PathPromptOptions, Render, SharedString, Styled, Window,
};
use nostr_sdk::prelude::*;
use smol::fs;
use theme::ActiveTheme;
use ui::button::{Button, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputState, TextInput};
use ui::popup_menu::PopupMenu;
use ui::{Disableable, Icon, IconName, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<NewAccount> {
    NewAccount::new(window, cx)
}

pub struct NewAccount {
    name_input: Entity<InputState>,
    avatar_input: Entity<InputState>,
    bio_input: Entity<InputState>,
    is_uploading: bool,
    is_submitting: bool,
    // Panel
    name: SharedString,
    closable: bool,
    zoomable: bool,
    focus_handle: FocusHandle,
}

impl NewAccount {
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::view(window, cx))
    }

    fn view(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Alice"));
        let avatar_input = cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/avatar.jpg"));
        let bio_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .placeholder("A short introduce about you.")
        });

        Self {
            name_input,
            avatar_input,
            bio_input,
            is_uploading: false,
            is_submitting: false,
            name: "New Account".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_submitting(true, cx);

        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();
        let bio = self.bio_input.read(cx).value().to_string();

        let mut metadata = Metadata::new().display_name(name).about(bio);

        if let Ok(url) = Url::from_str(&avatar) {
            metadata = metadata.picture(url);
        };

        AppState::global(cx).update(cx, |this, cx| {
            this.new_account(metadata, window, cx);
        });
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let avatar_input = self.avatar_input.downgrade();
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
        });

        self.set_uploading(true, cx);

        cx.spawn_in(window, async move |this, cx| {
            match Flatten::flatten(paths.await.map_err(|e| e.into())) {
                Ok(Some(mut paths)) => {
                    let Some(path) = paths.pop() else {
                        cx.update(|_, cx| {
                            this.update(cx, |this, cx| {
                                this.set_uploading(false, cx);
                            })
                            .ok();
                        })
                        .ok();

                        return;
                    };

                    if let Ok(file_data) = fs::read(path).await {
                        let (tx, rx) = oneshot::channel::<Url>();

                        spawn(async move {
                            if let Ok(url) = nip96_upload(&shared_state().client, file_data).await {
                                _ = tx.send(url);
                            }
                        });

                        if let Ok(url) = rx.await {
                            cx.update(|window, cx| {
                                // Stop loading spinner
                                this.update(cx, |this, cx| {
                                    this.set_uploading(false, cx);
                                })
                                .ok();

                                // Set avatar input
                                avatar_input
                                    .update(cx, |this, cx| {
                                        this.set_value(url.to_string(), window, cx);
                                    })
                                    .ok();
                            })
                            .ok();
                        }
                    }
                }
                Ok(None) => {
                    cx.update(|_, cx| {
                        this.update(cx, |this, cx| {
                            this.set_uploading(false, cx);
                        })
                    })
                    .ok();
                }
                Err(_) => {}
            }
        })
        .detach();
    }

    fn set_submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_submitting = status;
        cx.notify();
    }

    fn set_uploading(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_uploading = status;
        cx.notify();
    }
}

impl Panel for NewAccount {
    fn panel_id(&self) -> SharedString {
        self.name.clone()
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

impl EventEmitter<PanelEvent> for NewAccount {}

impl Focusable for NewAccount {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NewAccount {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_10()
            .child(
                div()
                    .text_center()
                    .text_lg()
                    .font_semibold()
                    .line_height(relative(1.3))
                    .child("Create New Account"),
            )
            .child(
                div()
                    .w_72()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .w_full()
                            .h_32()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .map(|this| {
                                if self.avatar_input.read(cx).value().is_empty() {
                                    this.child(img("brand/avatar.png").rounded_full().size_10().flex_shrink_0())
                                } else {
                                    this.child(
                                        img(self.avatar_input.read(cx).value().clone())
                                            .rounded_full()
                                            .size_10()
                                            .flex_shrink_0(),
                                    )
                                }
                            })
                            .child(
                                Button::new("upload")
                                    .label("Set Profile Picture")
                                    .icon(Icon::new(IconName::Plus))
                                    .ghost()
                                    .small()
                                    .disabled(self.is_submitting)
                                    .loading(self.is_uploading)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.upload(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_sm()
                            .child("Name *:")
                            .child(TextInput::new(&self.name_input).small()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .text_sm()
                            .child("Bio:")
                            .child(TextInput::new(&self.bio_input).small()),
                    )
                    .child(div().my_2().w_full().h_px().bg(cx.theme().elevated_surface_background))
                    .child(
                        Button::new("submit")
                            .label("Continue")
                            .primary()
                            .loading(self.is_submitting)
                            .disabled(self.is_submitting || self.is_uploading)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.submit(window, cx);
                            })),
                    ),
            )
    }
}
