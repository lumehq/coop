use anyhow::anyhow;
use common::nip96::nip96_upload;
use global::nostr_client;
use gpui::{
    div, relative, rems, AnyElement, App, AppContext, AsyncWindowContext, Context, Entity,
    EventEmitter, Flatten, FocusHandle, Focusable, IntoElement, ParentElement, PathPromptOptions,
    Render, SharedString, Styled, WeakEntity, Window,
};
use gpui_tokio::Tokio;
use i18n::t;
use identity::Identity;
use nostr_sdk::prelude::*;
use settings::AppSettings;
use smol::fs;
use theme::ActiveTheme;
use ui::avatar::Avatar;
use ui::button::{Button, ButtonRounded, ButtonVariants};
use ui::dock_area::panel::{Panel, PanelEvent};
use ui::input::{InputState, TextInput};
use ui::popup_menu::PopupMenu;
use ui::{divider, v_flex, ContextModal, Disableable, IconName, Sizable, StyledExt};

pub fn init(window: &mut Window, cx: &mut App) -> Entity<NewAccount> {
    NewAccount::new(window, cx)
}

pub struct NewAccount {
    name_input: Entity<InputState>,
    avatar_input: Entity<InputState>,
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
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(SharedString::new(t!("profile.placeholder_name")))
        });

        let avatar_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/avatar.png"));

        Self {
            name_input,
            avatar_input,
            is_uploading: false,
            is_submitting: false,
            name: "New Account".into(),
            closable: true,
            zoomable: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitting(true, cx);

        let identity = Identity::global(cx);
        let avatar = self.avatar_input.read(cx).value().to_string();
        let name = self.name_input.read(cx).value().to_string();

        // Build metadata
        let mut metadata = Metadata::new().display_name(name.clone()).name(name);

        if let Ok(url) = Url::parse(&avatar) {
            metadata = metadata.picture(url);
        };

        identity.update(cx, |this, cx| {
            this.new_identity(metadata, window, cx);
        });
    }

    fn upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.uploading(true, cx);

        // Get the user's configured NIP96 server
        let nip96_server = AppSettings::get_media_server(cx);

        // Open native file dialog
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
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
                    cx.update(|window, cx| {
                        this.update(cx, |this, cx| {
                            this.uploading(false, cx);
                            this.avatar_input.update(cx, |this, cx| {
                                this.set_value(url.to_string(), window, cx);
                            });
                        })
                        .ok();
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    Self::notify_error(cx, this, e.to_string());
                }
                Err(e) => {
                    Self::notify_error(cx, this, e.to_string());
                }
            }
        })
        .detach();
    }

    fn notify_error(cx: &mut AsyncWindowContext, entity: WeakEntity<NewAccount>, e: String) {
        cx.update(|window, cx| {
            entity
                .update(cx, |this, cx| {
                    window.push_notification(e, cx);
                    this.uploading(false, cx);
                })
                .ok();
        })
        .ok();
    }

    fn submitting(&mut self, status: bool, cx: &mut Context<Self>) {
        self.is_submitting = status;
        cx.notify();
    }

    fn uploading(&mut self, status: bool, cx: &mut Context<Self>) {
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .relative()
            .items_center()
            .justify_center()
            .gap_10()
            .child(
                div()
                    .text_lg()
                    .text_center()
                    .font_semibold()
                    .line_height(relative(1.3))
                    .child(SharedString::new(t!("new_account.title"))),
            )
            .child(
                v_flex()
                    .w_96()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .text_sm()
                            .child(SharedString::new(t!("new_account.name")))
                            .child(TextInput::new(&self.name_input).small()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .child(SharedString::new(t!("new_account.avatar"))),
                            )
                            .child(
                                v_flex()
                                    .p_1()
                                    .h_32()
                                    .w_full()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_dashed()
                                    .border_color(cx.theme().border)
                                    .child(
                                        Avatar::new(self.avatar_input.read(cx).value().to_string())
                                            .size(rems(2.25)),
                                    )
                                    .child(
                                        Button::new("upload")
                                            .icon(IconName::Plus)
                                            .label(t!("common.upload"))
                                            .ghost()
                                            .small()
                                            .rounded(ButtonRounded::Full)
                                            .disabled(self.is_submitting)
                                            .loading(self.is_uploading)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.upload(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(divider(cx))
                    .child(
                        Button::new("submit")
                            .label(SharedString::new(t!("common.continue")))
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
