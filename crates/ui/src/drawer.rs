use std::{rc::Rc, time::Duration};

use gpui::{
    actions, anchored, div, point, prelude::FluentBuilder as _, px, Animation, AnimationExt as _,
    AnyElement, AppContext, ClickEvent, DefiniteLength, DismissEvent, Div, EventEmitter,
    FocusHandle, InteractiveElement as _, IntoElement, KeyBinding, MouseButton, ParentElement,
    Pixels, RenderOnce, Styled, WindowContext,
};

use crate::{
    button::{Button, ButtonVariants as _},
    h_flex,
    modal::overlay_color,
    root::ContextModal as _,
    scroll::ScrollbarAxis,
    theme::ActiveTheme,
    title_bar::TITLE_BAR_HEIGHT,
    v_flex, IconName, Placement, Sizable, StyledExt as _,
};

actions!(drawer, [Escape]);

const CONTEXT: &str = "Drawer";

pub fn init(cx: &mut AppContext) {
    cx.bind_keys([KeyBinding::new("escape", Escape, Some(CONTEXT))])
}

type OnClose = Rc<dyn Fn(&ClickEvent, &mut WindowContext) + 'static>;

#[derive(IntoElement)]
pub struct Drawer {
    pub(crate) focus_handle: FocusHandle,
    placement: Placement,
    size: DefiniteLength,
    resizable: bool,
    on_close: OnClose,
    title: Option<AnyElement>,
    footer: Option<AnyElement>,
    content: Div,
    margin_top: Pixels,
    overlay: bool,
}

impl Drawer {
    pub fn new(cx: &mut WindowContext) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            placement: Placement::Right,
            size: DefiniteLength::Absolute(px(350.).into()),
            resizable: true,
            title: None,
            footer: None,
            content: v_flex().px_4().py_3(),
            margin_top: TITLE_BAR_HEIGHT,
            overlay: true,
            on_close: Rc::new(|_, _| {}),
        }
    }

    /// Sets the title of the drawer.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Set the footer of the drawer.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Sets the size of the drawer, default is 350px.
    pub fn size(mut self, size: impl Into<DefiniteLength>) -> Self {
        self.size = size.into();
        self
    }

    /// Sets the margin top of the drawer, default is 0px.
    ///
    /// This is used to let Drawer be placed below a Windows Title, you can give the height of the title bar.
    pub fn margin_top(mut self, top: Pixels) -> Self {
        self.margin_top = top;
        self
    }

    /// Sets the placement of the drawer, default is `Placement::Right`.
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the placement of the drawer, default is `Placement::Right`.
    pub fn set_placement(&mut self, placement: Placement) {
        self.placement = placement;
    }

    /// Sets whether the drawer is resizable, default is `true`.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether the drawer should have an overlay, default is `true`.
    pub fn overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Listen to the close event of the drawer.
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut WindowContext) + 'static,
    ) -> Self {
        self.on_close = Rc::new(on_close);
        self
    }
}

impl EventEmitter<DismissEvent> for Drawer {}
impl ParentElement for Drawer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}
impl Styled for Drawer {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.content.style()
    }
}

impl RenderOnce for Drawer {
    fn render(self, cx: &mut WindowContext) -> impl IntoElement {
        let placement = self.placement;
        let titlebar_height = self.margin_top;
        let size = cx.viewport_size();
        let on_close = self.on_close.clone();

        anchored()
            .position(point(px(0.), titlebar_height))
            .snap_to_window()
            .child(
                div()
                    .occlude()
                    .w(size.width)
                    .h(size.height - titlebar_height)
                    .bg(overlay_color(self.overlay, cx))
                    .when(self.overlay, |this| {
                        this.on_mouse_down(MouseButton::Left, {
                            let on_close = self.on_close.clone();
                            move |_, cx| {
                                on_close(&ClickEvent::default(), cx);
                                cx.close_drawer();
                            }
                        })
                    })
                    .child(
                        v_flex()
                            .id("drawer")
                            .key_context(CONTEXT)
                            .track_focus(&self.focus_handle)
                            .on_action({
                                let on_close = self.on_close.clone();
                                move |_: &Escape, cx| {
                                    on_close(&ClickEvent::default(), cx);
                                    cx.close_drawer();
                                }
                            })
                            .absolute()
                            .occlude()
                            .bg(cx.theme().background)
                            .border_color(cx.theme().border)
                            .shadow_xl()
                            .map(|this| {
                                // Set the size of the drawer.
                                if placement.is_horizontal() {
                                    this.h_full().w(self.size)
                                } else {
                                    this.w_full().h(self.size)
                                }
                            })
                            .map(|this| match self.placement {
                                Placement::Top => this.top_0().left_0().right_0().border_b_1(),
                                Placement::Right => this.top_0().right_0().bottom_0().border_l_1(),
                                Placement::Bottom => {
                                    this.bottom_0().left_0().right_0().border_t_1()
                                }
                                Placement::Left => this.top_0().left_0().bottom_0().border_r_1(),
                            })
                            .child(
                                // TitleBar
                                h_flex()
                                    .justify_between()
                                    .px_4()
                                    .py_2()
                                    .w_full()
                                    .child(self.title.unwrap_or(div().into_any_element()))
                                    .child(
                                        Button::new("close")
                                            .small()
                                            .ghost()
                                            .icon(IconName::Close)
                                            .on_click(move |_, cx| {
                                                on_close(&ClickEvent::default(), cx);
                                                cx.close_drawer();
                                            }),
                                    ),
                            )
                            .child(
                                // Body
                                div().flex_1().overflow_hidden().child(
                                    v_flex()
                                        .scrollable(
                                            cx.parent_view_id().unwrap_or_default(),
                                            ScrollbarAxis::Vertical,
                                        )
                                        .child(self.content),
                                ),
                            )
                            .when_some(self.footer, |this, footer| {
                                // Footer
                                this.child(
                                    h_flex()
                                        .justify_between()
                                        .px_4()
                                        .py_3()
                                        .w_full()
                                        .child(footer),
                                )
                            })
                            .with_animation(
                                "slide",
                                Animation::new(Duration::from_secs_f64(0.15)),
                                move |this, delta| {
                                    let y = px(-100.) + delta * px(100.);
                                    this.map(|this| match placement {
                                        Placement::Top => this.top(y),
                                        Placement::Right => this.right(y),
                                        Placement::Bottom => this.bottom(y),
                                        Placement::Left => this.left(y),
                                    })
                                },
                            ),
                    ),
            )
    }
}