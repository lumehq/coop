use super::PanelEvent;
use crate::{
    dock_area::panel::Panel,
    dock_area::state::PanelState,
    theme::{scale::ColorScaleStep, ActiveTheme},
};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, ParentElement as _, Render, SharedString,
    Styled as _, Window,
};

pub(crate) struct InvalidPanel {
    name: SharedString,
    focus_handle: FocusHandle,
    old_state: PanelState,
}

impl InvalidPanel {
    pub(crate) fn new(name: &str, state: PanelState, _window: &mut Window, cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            name: SharedString::from(name.to_owned()),
            old_state: state,
        }
    }
}

impl Panel for InvalidPanel {
    fn panel_id(&self) -> SharedString {
        "InvalidPanel".into()
    }

    fn dump(&self, _cx: &App) -> PanelState {
        self.old_state.clone()
    }
}

impl EventEmitter<PanelEvent> for InvalidPanel {}

impl Focusable for InvalidPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InvalidPanel {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
            .size_full()
            .my_6()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
            .child(format!(
                "The `{}` panel type is not registered in PanelRegistry.",
                self.name.clone()
            ))
    }
}
