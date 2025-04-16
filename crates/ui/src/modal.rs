use crate::{
    animation::cubic_bezier,
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    theme::{scale::ColorScaleStep, ActiveTheme as _},
    v_flex, ContextModal, IconName, Sizable as _, StyledExt,
};
use gpui::{
    actions, anchored, div, point, prelude::FluentBuilder, px, Animation, AnimationExt as _,
    AnyElement, App, Bounds, ClickEvent, Div, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, MouseButton, ParentElement, Pixels, Point, RenderOnce, SharedString, Styled,
    Window,
};
use std::{rc::Rc, time::Duration};

actions!(modal, [Escape]);

const CONTEXT: &str = "Modal";

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Escape, Some(CONTEXT))])
}

type OnClose = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Modal {
    base: Div,
    title: Option<AnyElement>,
    footer: Option<AnyElement>,
    content: Div,
    width: Pixels,
    max_width: Option<Pixels>,
    margin_top: Option<Pixels>,
    on_close: OnClose,
    closable: bool,
    keyboard: bool,
    /// This will be change when open the modal, the focus handle is create when open the modal.
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layer_ix: usize,
    pub(crate) overlay: bool,
}

impl Modal {
    pub fn new(_window: &mut Window, cx: &mut App) -> Self {
        let base = v_flex()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().base.step(cx, ColorScaleStep::SEVEN))
            .rounded_xl()
            .shadow_md();

        Self {
            base,
            focus_handle: cx.focus_handle(),
            title: None,
            footer: None,
            content: v_flex(),
            margin_top: None,
            width: px(480.),
            max_width: None,
            overlay: true,
            keyboard: true,
            closable: true,
            layer_ix: 0,
            on_close: Rc::new(|_, _, _| {}),
        }
    }

    /// Sets the title of the modal.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Set the footer of the modal.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Sets the callback for when the modal is closed.
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Rc::new(on_close);
        self
    }

    /// Sets the false to make modal unclosable, default: true
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// Set the top offset of the modal, defaults to None, will use the 1/10 of the viewport height.
    pub fn margin_top(mut self, margin_top: Pixels) -> Self {
        self.margin_top = Some(margin_top);
        self
    }

    /// Sets the width of the modal, defaults to 480px.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Set the maximum width of the modal, defaults to `None`.
    pub fn max_w(mut self, max_width: Pixels) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Set the overlay of the modal, defaults to `true`.
    pub fn overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Set whether to support keyboard esc to close the modal, defaults to `true`.
    pub fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl Styled for Modal {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl gpui::IntoElement {
        let layer_ix = self.layer_ix;
        let on_close = self.on_close.clone();
        let window_paddings = crate::window_border::window_paddings(window, cx);
        let view_size = window.viewport_size()
            - gpui::size(
                window_paddings.left + window_paddings.right,
                window_paddings.top + window_paddings.bottom,
            );
        let bounds = Bounds {
            origin: Point::default(),
            size: view_size,
        };
        let offset_top = px(layer_ix as f32 * 2.);
        let y = self.margin_top.unwrap_or(view_size.height / 16.) + offset_top;
        let x = bounds.center().x - self.width / 2.;

        anchored()
            .position(point(window_paddings.left, window_paddings.top))
            .snap_to_window()
            .child(
                div()
                    .occlude()
                    .w(view_size.width)
                    .h(view_size.height)
                    .when(self.overlay, |this| {
                        this.bg(cx.theme().base.step_alpha(cx, ColorScaleStep::EIGHT))
                    })
                    .when(self.closable, |this| {
                        this.on_mouse_down(MouseButton::Left, {
                            let on_close = self.on_close.clone();
                            move |_, window, cx| {
                                on_close(&ClickEvent::default(), window, cx);
                                window.close_modal(cx);
                            }
                        })
                    })
                    .child(
                        self.base
                            .id(SharedString::from(format!("modal-{layer_ix}")))
                            .key_context(CONTEXT)
                            .track_focus(&self.focus_handle)
                            .absolute()
                            .occlude()
                            .relative()
                            .left(x)
                            .top(y)
                            .w(self.width)
                            .when_some(self.max_width, |this, w| this.max_w(w))
                            .px_4()
                            .pb_4()
                            .child(
                                div()
                                    .h_12()
                                    .mb_2()
                                    .border_b_1()
                                    .border_color(cx.theme().base.step(cx, ColorScaleStep::SIX))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .when_some(self.title, |this, title| {
                                        this.child(div().font_semibold().child(title))
                                    })
                                    .when(self.closable, |this| {
                                        this.child(
                                            Button::new(SharedString::from(format!(
                                                "modal-close-{layer_ix}"
                                            )))
                                            .small()
                                            .icon(IconName::CloseCircleFill)
                                            .custom(
                                                ButtonCustomVariant::new(window, cx)
                                                    .foreground(
                                                        cx.theme()
                                                            .base
                                                            .step(cx, ColorScaleStep::NINE),
                                                    )
                                                    .color(cx.theme().transparent)
                                                    .hover(cx.theme().transparent)
                                                    .active(cx.theme().transparent)
                                                    .border(cx.theme().transparent),
                                            )
                                            .on_click(move |_, window, cx| {
                                                on_close(&ClickEvent::default(), window, cx);
                                                window.close_modal(cx);
                                            }),
                                        )
                                    }),
                            )
                            .child(self.content)
                            .children(self.footer)
                            .when(self.keyboard, |this| {
                                this.on_action({
                                    let on_close = self.on_close.clone();
                                    move |_: &Escape, window, cx| {
                                        // FIXME:
                                        //
                                        // Here some Modal have no focus_handle, so it will not work will Escape key.
                                        // But by now, we `cx.close_modal()` going to close the last active model, so the Escape is unexpected to work.
                                        on_close(&ClickEvent::default(), window, cx);
                                        window.close_modal(cx);
                                    }
                                })
                            })
                            .with_animation(
                                "slide-down",
                                Animation::new(Duration::from_secs_f64(0.25))
                                    .with_easing(cubic_bezier(0.32, 0.72, 0., 1.)),
                                move |this, delta| {
                                    let y_offset = px(0.) + delta * px(30.);
                                    this.top(y + y_offset)
                                },
                            ),
                    ),
            )
    }
}
