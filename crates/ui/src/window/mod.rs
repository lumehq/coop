use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, point, px, AnyView, App, AppContext, Bounds, Context, CursorStyle, Decorations,
    Edges, Entity, FocusHandle, HitboxBehavior, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement as _, Pixels, Point, Render, ResizeEdge, Size, Styled, Window,
};
use theme::{
    ActiveTheme, CLIENT_SIDE_DECORATION_ROUNDING, CLIENT_SIDE_DECORATION_SHADOW,
    DECORATION_BORDER_SIZE,
};

use crate::input::InputState;
use crate::modal::Modal;
use crate::notification::{Notification, NotificationList};

/// Extension trait for [`WindowContext`] and [`ViewContext`] to add drawer functionality.
pub trait ContextModal: Sized {
    /// Opens a Modal.
    fn open_modal<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Modal, &mut Window, &mut App) -> Modal + 'static;

    /// Return true, if there is an active Modal.
    fn has_active_modal(&mut self, cx: &mut App) -> bool;

    /// Closes the last active Modal.
    fn close_modal(&mut self, cx: &mut App);

    /// Closes all active Modals.
    fn close_all_modals(&mut self, cx: &mut App);

    /// Returns number of notifications.
    fn notifications(&mut self, cx: &mut App) -> Rc<Vec<Entity<Notification>>>;

    /// Pushes a notification to the notification list.
    fn push_notification(&mut self, note: impl Into<Notification>, cx: &mut App);

    /// Clear all notifications
    fn clear_notifications(&mut self, cx: &mut App);

    /// Return current focused Input entity.
    fn focused_input(&mut self, cx: &mut App) -> Option<Entity<InputState>>;

    /// Returns true if there is a focused Input entity.
    fn has_focused_input(&mut self, cx: &mut App) -> bool;
}

impl ContextModal for Window {
    fn open_modal<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Modal, &mut Window, &mut App) -> Modal + 'static,
    {
        Root::update(self, cx, move |root, window, cx| {
            // Only save focus handle if there are no active modals.
            // This is used to restore focus when all modals are closed.
            if root.active_modals.is_empty() {
                root.previous_focus_handle = window.focused(cx);
            }

            let focus_handle = cx.focus_handle();
            focus_handle.focus(window);

            root.active_modals.push(ActiveModal {
                focus_handle,
                builder: Rc::new(build),
            });

            cx.notify();
        })
    }

    fn has_active_modal(&mut self, cx: &mut App) -> bool {
        !Root::read(self, cx).active_modals.is_empty()
    }

    fn close_modal(&mut self, cx: &mut App) {
        Root::update(self, cx, move |root, window, cx| {
            root.active_modals.pop();

            if let Some(top_modal) = root.active_modals.last() {
                // Focus the next modal.
                top_modal.focus_handle.focus(window);
            } else {
                // Restore focus if there are no more modals.
                root.focus_back(window, cx);
            }
            cx.notify();
        })
    }

    fn close_all_modals(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.active_modals.clear();
            root.focus_back(window, cx);
            cx.notify();
        })
    }

    fn push_notification(&mut self, note: impl Into<Notification>, cx: &mut App) {
        let note = note.into();
        Root::update(self, cx, move |root, window, cx| {
            root.notification
                .update(cx, |view, cx| view.push(note, window, cx));
            cx.notify();
        })
    }

    fn clear_notifications(&mut self, cx: &mut App) {
        Root::update(self, cx, move |root, window, cx| {
            root.notification
                .update(cx, |view, cx| view.clear(window, cx));
            cx.notify();
        })
    }

    fn notifications(&mut self, cx: &mut App) -> Rc<Vec<Entity<Notification>>> {
        let entity = Root::read(self, cx).notification.clone();
        Rc::new(entity.read(cx).notifications())
    }

    fn has_focused_input(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).focused_input.is_some()
    }

    fn focused_input(&mut self, cx: &mut App) -> Option<Entity<InputState>> {
        Root::read(self, cx).focused_input.clone()
    }
}

type Builder = Rc<dyn Fn(Modal, &mut Window, &mut App) -> Modal + 'static>;

#[derive(Clone)]
pub(crate) struct ActiveModal {
    focus_handle: FocusHandle,
    builder: Builder,
}

/// Root is a view for the App window for as the top level view (Must be the first view in the window).
///
/// It is used to manage the Modal, and Notification.
pub struct Root {
    pub(crate) active_modals: Vec<ActiveModal>,
    pub notification: Entity<NotificationList>,
    pub focused_input: Option<Entity<InputState>>,
    /// Used to store the focus handle of the previous view.
    ///
    /// When the Modal closes, we will focus back to the previous view.
    previous_focus_handle: Option<FocusHandle>,
    view: AnyView,
}

impl Root {
    pub fn new(view: AnyView, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            previous_focus_handle: None,
            focused_input: None,
            active_modals: Vec::new(),
            notification: cx.new(|cx| NotificationList::new(window, cx)),
            view,
        }
    }

    pub fn update<F>(window: &mut Window, cx: &mut App, f: F)
    where
        F: FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        if let Some(Some(root)) = window.root::<Root>() {
            root.update(cx, |root, cx| f(root, window, cx));
        }
    }

    pub fn read<'a>(window: &'a mut Window, cx: &'a mut App) -> &'a Self {
        window
            .root::<Root>()
            .expect("The window root view should be of type `ui::Root`.")
            .unwrap()
            .read(cx)
    }

    fn focus_back(&mut self, window: &mut Window, _: &mut App) {
        if let Some(handle) = self.previous_focus_handle.clone() {
            window.focus(&handle);
        }
    }

    // Render Notification layer.
    pub fn render_notification_layer(
        window: &mut Window,
        cx: &mut App,
    ) -> Option<impl IntoElement> {
        let root = window.root::<Root>()??;

        Some(div().child(root.read(cx).notification.clone()))
    }

    /// Render the Modal layer.
    pub fn render_modal_layer(window: &mut Window, cx: &mut App) -> Option<impl IntoElement> {
        let root = window.root::<Root>()??;

        let active_modals = root.read(cx).active_modals.clone();

        if active_modals.is_empty() {
            return None;
        }

        let mut show_overlay_ix = None;

        let mut modals = active_modals
            .iter()
            .enumerate()
            .map(|(i, active_modal)| {
                let mut modal = Modal::new(window, cx);

                modal = (active_modal.builder)(modal, window, cx);

                // Give the modal the focus handle, because `modal` is a temporary value, is not possible to
                // keep the focus handle in the modal.
                //
                // So we keep the focus handle in the `active_modal`, this is owned by the `Root`.
                modal.focus_handle = active_modal.focus_handle.clone();

                modal.layer_ix = i;
                // Find the modal which one needs to show overlay.
                if modal.has_overlay() {
                    show_overlay_ix = Some(i);
                }

                modal
            })
            .collect::<Vec<_>>();

        if let Some(ix) = show_overlay_ix {
            if let Some(modal) = modals.get_mut(ix) {
                modal.overlay_visible = true;
            }
        }

        Some(div().children(modals))
    }

    /// Return the root view of the Root.
    pub fn view(&self) -> &AnyView {
        &self.view
    }

    /// Replace the root view of the Root.
    pub fn replace_view(&mut self, view: AnyView) {
        self.view = view;
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let decorations = window.window_decorations();
        let base_font_size = cx.theme().font_size;

        window.set_client_inset(CLIENT_SIDE_DECORATION_SHADOW);
        window.set_rem_size(base_font_size);

        div()
            .id("window-frame")
            .size_full()
            .map(|div| match decorations {
                Decorations::Server => div,
                Decorations::Client { tiling, .. } => div
                    .bg(gpui::transparent_black())
                    .child(
                        canvas(
                            |_bounds, window, _cx| {
                                window.insert_hitbox(
                                    Bounds::new(
                                        point(px(0.0), px(0.0)),
                                        window.window_bounds().get_bounds().size,
                                    ),
                                    HitboxBehavior::Normal,
                                )
                            },
                            move |_bounds, hitbox, window, _cx| {
                                let mouse = window.mouse_position();
                                let size = window.window_bounds().get_bounds().size;
                                let Some(edge) =
                                    resize_edge(mouse, CLIENT_SIDE_DECORATION_SHADOW, size)
                                else {
                                    return;
                                };
                                window.set_cursor_style(
                                    match edge {
                                        ResizeEdge::Top | ResizeEdge::Bottom => {
                                            CursorStyle::ResizeUpDown
                                        }
                                        ResizeEdge::Left | ResizeEdge::Right => {
                                            CursorStyle::ResizeLeftRight
                                        }
                                        ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                            CursorStyle::ResizeUpLeftDownRight
                                        }
                                        ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                            CursorStyle::ResizeUpRightDownLeft
                                        }
                                    },
                                    &hitbox,
                                );
                            },
                        )
                        .size_full()
                        .absolute(),
                    )
                    .when(!(tiling.top || tiling.right), |div| {
                        div.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |div| {
                        div.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.right), |div| {
                        div.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.left), |div| {
                        div.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!tiling.top, |div| div.pt(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.bottom, |div| div.pb(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.left, |div| div.pl(CLIENT_SIDE_DECORATION_SHADOW))
                    .when(!tiling.right, |div| div.pr(CLIENT_SIDE_DECORATION_SHADOW))
                    .on_mouse_down(MouseButton::Left, move |_, window, _cx| {
                        let size = window.window_bounds().get_bounds().size;
                        let pos = window.mouse_position();

                        if let Some(edge) = resize_edge(pos, CLIENT_SIDE_DECORATION_SHADOW, size) {
                            window.start_window_resize(edge)
                        };
                    }),
            })
            .child(
                div()
                    .size_full()
                    .relative()
                    .font_family(".SystemUIFont")
                    .bg(cx.theme().background)
                    .text_color(cx.theme().text)
                    .map(|div| match decorations {
                        Decorations::Server => div,
                        Decorations::Client { tiling } => div
                            .border_color(cx.theme().window_border)
                            .when(!(tiling.top || tiling.right), |div| {
                                div.rounded_tr(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.top || tiling.left), |div| {
                                div.rounded_tl(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.right), |div| {
                                div.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!(tiling.bottom || tiling.left), |div| {
                                div.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
                            })
                            .when(!tiling.top, |div| div.border_t(DECORATION_BORDER_SIZE))
                            .when(!tiling.bottom, |div| div.border_b(DECORATION_BORDER_SIZE))
                            .when(!tiling.left, |div| div.border_l(DECORATION_BORDER_SIZE))
                            .when(!tiling.right, |div| div.border_r(DECORATION_BORDER_SIZE))
                            .when(!tiling.is_tiled(), |div| {
                                div.shadow(vec![gpui::BoxShadow {
                                    color: Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.3,
                                    },
                                    blur_radius: CLIENT_SIDE_DECORATION_SHADOW / 2.,
                                    spread_radius: px(0.),
                                    offset: point(px(0.0), px(0.0)),
                                }])
                            }),
                    })
                    .on_mouse_move(|_e, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(self.view.clone()),
            )
    }
}

/// Determines resize edge
pub(crate) fn resize_edge(
    pos: Point<Pixels>,
    shadow_size: Pixels,
    size: Size<Pixels>,
) -> Option<ResizeEdge> {
    let edge = if pos.y < shadow_size && pos.x < shadow_size {
        ResizeEdge::TopLeft
    } else if pos.y < shadow_size && pos.x > size.width - shadow_size {
        ResizeEdge::TopRight
    } else if pos.y < shadow_size {
        ResizeEdge::Top
    } else if pos.y > size.height - shadow_size && pos.x < shadow_size {
        ResizeEdge::BottomLeft
    } else if pos.y > size.height - shadow_size && pos.x > size.width - shadow_size {
        ResizeEdge::BottomRight
    } else if pos.y > size.height - shadow_size {
        ResizeEdge::Bottom
    } else if pos.x < shadow_size {
        ResizeEdge::Left
    } else if pos.x > size.width - shadow_size {
        ResizeEdge::Right
    } else {
        return None;
    };
    Some(edge)
}

/// Get the window paddings.
pub(crate) fn window_paddings(window: &Window, _cx: &App) -> Edges<Pixels> {
    match window.window_decorations() {
        Decorations::Server => Edges::all(px(0.0)),
        Decorations::Client { tiling } => {
            let mut paddings = Edges::all(CLIENT_SIDE_DECORATION_SHADOW);
            if tiling.top {
                paddings.top = px(0.0);
            }
            if tiling.bottom {
                paddings.bottom = px(0.0);
            }
            if tiling.left {
                paddings.left = px(0.0);
            }
            if tiling.right {
                paddings.right = px(0.0);
            }
            paddings
        }
    }
}
