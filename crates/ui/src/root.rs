use std::rc::Rc;

use gpui::{
    div, AnyView, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement as _, Render, Styled, Window,
};
use theme::ActiveTheme;

use crate::input::InputState;
use crate::modal::Modal;
use crate::notification::{Notification, NotificationList};
use crate::window_border;

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
        let base_font_size = cx.theme().font_size;
        window.set_rem_size(base_font_size);

        window_border().child(
            div()
                .id("root")
                .relative()
                .size_full()
                .font_family(".SystemUIFont")
                .bg(cx.theme().background)
                .text_color(cx.theme().text)
                .child(self.view.clone()),
        )
    }
}
