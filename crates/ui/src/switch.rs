use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, Animation, AnimationExt as _, AnyElement, App, Element, ElementId, GlobalElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement as _, SharedString, Styled as _,
    Window,
};
use theme::ActiveTheme;

use crate::{h_flex, Disableable, Side, Sizable, Size};

type OnClick = Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>;

pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    label: Option<SharedString>,
    label_side: Side,
    on_click: OnClick,
    size: Size,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id: ElementId = id.into();
        Self {
            id: id.clone(),
            checked: false,
            disabled: false,
            label: None,
            on_click: None,
            label_side: Side::Right,
            size: Size::Medium,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&bool, &mut Window, &mut App) + 'static,
    {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub fn label_side(mut self, label_side: Side) -> Self {
        self.label_side = label_side;
        self
    }
}

impl Sizable for Switch {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Switch {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[derive(Default)]
pub struct SwitchState {
    prev_checked: Rc<RefCell<Option<bool>>>,
}

impl Element for Switch {
    type PrepaintState = ();
    type RequestLayoutState = AnyElement;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        window.with_element_state::<SwitchState, _>(global_id.unwrap(), move |state, window| {
            let state = state.unwrap_or_default();

            let theme = cx.theme();
            let checked = self.checked;
            let on_click = self.on_click.clone();

            let (bg, toggle_bg) = match self.checked {
                true => (theme.icon_accent, theme.background),
                false => (theme.element_background, theme.background),
            };

            let (bg, toggle_bg) = match self.disabled {
                true => (bg.opacity(0.3), toggle_bg.opacity(0.8)),
                false => (bg, toggle_bg),
            };

            let (bg_width, bg_height) = match self.size {
                Size::XSmall | Size::Small => (px(28.), px(16.)),
                _ => (px(36.), px(20.)),
            };

            let bar_width = match self.size {
                Size::XSmall | Size::Small => px(12.),
                _ => px(16.),
            };

            let inset = px(2.);

            let mut element = div()
                .flex()
                .child(
                    h_flex()
                        .id(self.id.clone())
                        .items_center()
                        .gap_2()
                        .when(self.label_side.is_left(), |this| this.flex_row_reverse())
                        .child(
                            // Switch Bar
                            div()
                                .id(self.id.clone())
                                .w(bg_width)
                                .h(bg_height)
                                .rounded(bg_height / 2.)
                                .flex()
                                .items_center()
                                .border(inset)
                                .border_color(theme.border_transparent)
                                .bg(bg)
                                .when(!self.disabled, |this| this.cursor_pointer())
                                .child(
                                    // Switch Toggle
                                    div().rounded_full().bg(toggle_bg).size(bar_width).map(
                                        |this| {
                                            let prev_checked = state.prev_checked.clone();
                                            if !self.disabled
                                                && prev_checked
                                                    .borrow()
                                                    .is_some_and(|prev| prev != checked)
                                            {
                                                let dur = Duration::from_secs_f64(0.15);
                                                cx.spawn(async move |cx| {
                                                    cx.background_executor().timer(dur).await;
                                                    *prev_checked.borrow_mut() = Some(checked);
                                                })
                                                .detach();
                                                this.with_animation(
                                                    ElementId::NamedInteger(
                                                        "move".into(),
                                                        checked as u64,
                                                    ),
                                                    Animation::new(dur),
                                                    move |this, delta| {
                                                        let max_x =
                                                            bg_width - bar_width - inset * 2;
                                                        let x = if checked {
                                                            max_x * delta
                                                        } else {
                                                            max_x - max_x * delta
                                                        };
                                                        this.left(x)
                                                    },
                                                )
                                                .into_any_element()
                                            } else {
                                                let max_x = bg_width - bar_width - inset * 2;
                                                let x = if checked { max_x } else { px(0.) };
                                                this.left(x).into_any_element()
                                            }
                                        },
                                    ),
                                ),
                        )
                        .when_some(self.label.clone(), |this, label| {
                            this.child(div().child(label).map(|this| match self.size {
                                Size::XSmall | Size::Small => this.text_sm(),
                                _ => this.text_base(),
                            }))
                        })
                        .when_some(
                            on_click
                                .as_ref()
                                .map(|c| c.clone())
                                .filter(|_| !self.disabled),
                            |this, on_click| {
                                let prev_checked = state.prev_checked.clone();
                                this.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    *prev_checked.borrow_mut() = Some(checked);
                                    on_click(&!checked, window, cx);
                                })
                            },
                        ),
                )
                .into_any_element();

            ((element.request_layout(window, cx), element), state)
        })
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        element: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx)
    }
}
