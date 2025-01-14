use super::PanelEvent;
use crate::{
    dock_area::panel::Panel,
    dock_area::state::PanelState,
    theme::{scale::ColorScaleStep, ActiveTheme},
};
use gpui::{
    AppContext, EventEmitter, FocusHandle, FocusableView, ParentElement as _, Render, SharedString,
    Styled as _, WindowContext,
};

pub(crate) struct InvalidPanel {
    name: SharedString,
    focus_handle: FocusHandle,
    old_state: PanelState,
}

impl InvalidPanel {
    pub(crate) fn new(name: &str, state: PanelState, cx: &mut WindowContext) -> Self {
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

    fn dump(&self, _cx: &AppContext) -> PanelState {
        self.old_state.clone()
    }
}

impl EventEmitter<PanelEvent> for InvalidPanel {}

impl FocusableView for InvalidPanel {
    fn focus_handle(&self, _: &AppContext) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for InvalidPanel {
    fn render(&mut self, cx: &mut gpui::ViewContext<Self>) -> impl gpui::IntoElement {
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
