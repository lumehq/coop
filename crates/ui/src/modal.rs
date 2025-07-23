use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{
    anchored, div, hsla, point, px, relative, Animation, AnimationExt as _, AnyElement, App,
    Bounds, BoxShadow, ClickEvent, Div, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    MouseButton, ParentElement, Pixels, Point, RenderOnce, SharedString, Styled, Window,
};
use theme::ActiveTheme;

use crate::actions::{Cancel, Confirm};
use crate::animation::cubic_bezier;
use crate::button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants as _};
use crate::{h_flex, v_flex, ContextModal, IconName, Root, Sizable, StyledExt};

const CONTEXT: &str = "Modal";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
    ]);
}

type OnClose = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type OnOk = Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static>>;
type OnCancel = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static>;
type RenderButtonFn = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;
type FooterFn =
    Box<dyn Fn(RenderButtonFn, RenderButtonFn, &mut Window, &mut App) -> Vec<AnyElement>>;

/// Modal button props.
pub struct ModalButtonProps {
    ok_text: Option<SharedString>,
    ok_variant: ButtonVariant,
    cancel_text: Option<SharedString>,
    cancel_variant: ButtonVariant,
}

impl Default for ModalButtonProps {
    fn default() -> Self {
        Self {
            ok_text: None,
            ok_variant: ButtonVariant::Primary,
            cancel_text: None,
            cancel_variant: ButtonVariant::Ghost,
        }
    }
}

impl ModalButtonProps {
    /// Sets the text of the OK button. Default is `OK`.
    pub fn ok_text(mut self, ok_text: impl Into<SharedString>) -> Self {
        self.ok_text = Some(ok_text.into());
        self
    }

    /// Sets the variant of the OK button. Default is `ButtonVariant::Primary`.
    pub fn ok_variant(mut self, ok_variant: ButtonVariant) -> Self {
        self.ok_variant = ok_variant;
        self
    }

    /// Sets the text of the Cancel button. Default is `Cancel`.
    pub fn cancel_text(mut self, cancel_text: impl Into<SharedString>) -> Self {
        self.cancel_text = Some(cancel_text.into());
        self
    }

    /// Sets the variant of the Cancel button. Default is `ButtonVariant::default()`.
    pub fn cancel_variant(mut self, cancel_variant: ButtonVariant) -> Self {
        self.cancel_variant = cancel_variant;
        self
    }
}

#[derive(IntoElement)]
pub struct Modal {
    base: Div,
    title: Option<AnyElement>,
    footer: Option<FooterFn>,
    content: Div,
    width: Pixels,
    max_width: Option<Pixels>,
    margin_top: Option<Pixels>,

    on_close: OnClose,
    on_ok: OnOk,
    on_cancel: OnCancel,
    button_props: ModalButtonProps,
    show_close: bool,
    overlay: bool,
    overlay_closable: bool,
    keyboard: bool,

    /// This will be change when open the modal, the focus handle is create when open the modal.
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layer_ix: usize,
    pub(crate) overlay_visible: bool,
}

impl Modal {
    pub fn new(_window: &mut Window, cx: &mut App) -> Self {
        let radius = (cx.theme().radius * 2.).min(px(20.));

        let base = v_flex()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(radius)
            .shadow_xl()
            .min_h_24();

        Self {
            base,
            focus_handle: cx.focus_handle(),
            title: None,
            footer: None,
            content: v_flex(),
            margin_top: None,
            width: px(380.),
            max_width: None,
            overlay: true,
            keyboard: true,
            layer_ix: 0,
            overlay_visible: false,
            on_close: Rc::new(|_, _, _| {}),
            on_ok: None,
            on_cancel: Rc::new(|_, _, _| true),
            button_props: ModalButtonProps::default(),
            show_close: true,
            overlay_closable: true,
        }
    }

    /// Sets the title of the modal.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Set the footer of the modal.
    ///
    /// The `footer` is a function that takes two `RenderButtonFn` and a `WindowContext` and returns a list of `AnyElement`.
    ///
    /// - First `RenderButtonFn` is the render function for the OK button.
    /// - Second `RenderButtonFn` is the render function for the CANCEL button.
    ///
    /// When you set the footer, the footer will be placed default footer buttons.
    pub fn footer<E, F>(mut self, footer: F) -> Self
    where
        E: IntoElement,
        F: Fn(RenderButtonFn, RenderButtonFn, &mut Window, &mut App) -> Vec<E> + 'static,
    {
        self.footer = Some(Box::new(move |ok, cancel, window, cx| {
            footer(ok, cancel, window, cx)
                .into_iter()
                .map(|e| e.into_any_element())
                .collect()
        }));
        self
    }

    /// Set to use confirm modal, with OK and Cancel buttons.
    ///
    /// See also [`Self::alert`]
    pub fn confirm(self) -> Self {
        self.footer(|ok, cancel, window, cx| vec![cancel(window, cx), ok(window, cx)])
            .overlay_closable(false)
            .show_close(false)
    }

    /// Set to as a alter modal, with OK button.
    ///
    /// See also [`Self::confirm`]
    pub fn alert(self) -> Self {
        self.footer(|ok, _, window, cx| vec![ok(window, cx)])
            .overlay_closable(false)
            .show_close(false)
    }

    /// Set the button props of the modal.
    pub fn button_props(mut self, button_props: ModalButtonProps) -> Self {
        self.button_props = button_props;
        self
    }

    /// Sets the callback for when the modal is closed.
    ///
    /// Called after [`Self::on_ok`] or [`Self::on_cancel`] callback.
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Rc::new(on_close);
        self
    }

    /// Sets the callback for when the modal is has been confirmed.
    ///
    /// The callback should return `true` to close the modal, if return `false` the modal will not be closed.
    pub fn on_ok(
        mut self,
        on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_ok = Some(Rc::new(on_ok));
        self
    }

    /// Sets the callback for when the modal is has been canceled.
    ///
    /// The callback should return `true` to close the modal, if return `false` the modal will not be closed.
    pub fn on_cancel(
        mut self,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_cancel = Rc::new(on_cancel);
        self
    }

    /// Sets the false to hide close icon, default: true
    pub fn show_close(mut self, show_close: bool) -> Self {
        self.show_close = show_close;
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

    /// Set the overlay closable of the modal, defaults to `true`.
    ///
    /// When the overlay is clicked, the modal will be closed.
    pub fn overlay_closable(mut self, overlay_closable: bool) -> Self {
        self.overlay_closable = overlay_closable;
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
        let on_ok = self.on_ok.clone();
        let on_cancel = self.on_cancel.clone();

        let render_ok: RenderButtonFn = Box::new({
            let on_ok = on_ok.clone();
            let on_close = on_close.clone();
            let ok_variant = self.button_props.ok_variant;
            let ok_text = self.button_props.ok_text.unwrap_or_else(|| "OK".into());

            move |_, _| {
                Button::new("ok")
                    .label(ok_text)
                    .with_variant(ok_variant)
                    .small()
                    .flex_1()
                    .on_click({
                        let on_ok = on_ok.clone();
                        let on_close = on_close.clone();

                        move |_, window, cx| {
                            if let Some(on_ok) = &on_ok {
                                if !on_ok(&ClickEvent::default(), window, cx) {
                                    return;
                                }
                            }

                            on_close(&ClickEvent::default(), window, cx);
                            window.close_modal(cx);
                        }
                    })
                    .into_any_element()
            }
        });

        let render_cancel: RenderButtonFn = Box::new({
            let on_cancel = on_cancel.clone();
            let on_close = on_close.clone();
            let cancel_variant = self.button_props.cancel_variant;
            let cancel_text = self
                .button_props
                .cancel_text
                .unwrap_or_else(|| "Cancel".into());

            move |_, _| {
                Button::new("cancel")
                    .label(cancel_text)
                    .with_variant(cancel_variant)
                    .small()
                    .flex_1()
                    .on_click({
                        let on_cancel = on_cancel.clone();
                        let on_close = on_close.clone();
                        move |_, window, cx| {
                            if !on_cancel(&ClickEvent::default(), window, cx) {
                                return;
                            }

                            on_close(&ClickEvent::default(), window, cx);
                            window.close_modal(cx);
                        }
                    })
                    .into_any_element()
            }
        });

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

        let offset_top = px(layer_ix as f32 * 16.);
        let y = self.margin_top.unwrap_or(view_size.height / 10.) + offset_top;
        let x = bounds.center().x - self.width / 2.;

        let animation = Animation::new(Duration::from_secs_f64(0.25))
            .with_easing(cubic_bezier(0.32, 0.72, 0., 1.));

        anchored()
            .position(point(window_paddings.left, window_paddings.top))
            .snap_to_window()
            .child(
                div()
                    .w(view_size.width)
                    .h(view_size.height)
                    .when(self.overlay_visible, |this| {
                        this.occlude().bg(cx.theme().overlay)
                    })
                    .when(self.overlay_closable, |this| {
                        // Only the last modal owns the `mouse down - close modal` event.
                        if (self.layer_ix + 1) != Root::read(window, cx).active_modals.len() {
                            return this;
                        }

                        this.on_mouse_down(MouseButton::Left, {
                            let on_cancel = on_cancel.clone();
                            let on_close = on_close.clone();
                            move |_, window, cx| {
                                on_cancel(&ClickEvent::default(), window, cx);
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
                            .when(self.keyboard, |this| {
                                this.on_action({
                                    let on_cancel = on_cancel.clone();
                                    let on_close = on_close.clone();
                                    move |_: &Cancel, window, cx| {
                                        // FIXME:
                                        //
                                        // Here some Modal have no focus_handle, so it will not work will Escape key.
                                        // But by now, we `cx.close_modal()` going to close the last active model, so the Escape is unexpected to work.
                                        on_cancel(&ClickEvent::default(), window, cx);
                                        on_close(&ClickEvent::default(), window, cx);
                                        window.close_modal(cx);
                                    }
                                })
                                .on_action({
                                    let on_ok = on_ok.clone();
                                    let on_close = on_close.clone();
                                    let has_footer = self.footer.is_some();
                                    move |_: &Confirm, window, cx| {
                                        if let Some(on_ok) = &on_ok {
                                            if on_ok(&ClickEvent::default(), window, cx) {
                                                on_close(&ClickEvent::default(), window, cx);
                                                window.close_modal(cx);
                                            }
                                        } else if has_footer {
                                            window.close_modal(cx);
                                        }
                                    }
                                })
                            })
                            .absolute()
                            .occlude()
                            .relative()
                            .left(x)
                            .top(y)
                            .w(self.width)
                            .when_some(self.max_width, |this, w| this.max_w(w))
                            .when_some(self.title, |this, title| {
                                this.child(
                                    div()
                                        .h_12()
                                        .px_3()
                                        .mb_2()
                                        .flex()
                                        .items_center()
                                        .font_semibold()
                                        .border_b_1()
                                        .border_color(cx.theme().border)
                                        .line_height(relative(1.))
                                        .child(title),
                                )
                            })
                            .when(self.show_close, |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "modal-close-{layer_ix}"
                                    )))
                                    .icon(IconName::CloseCircleFill)
                                    .absolute()
                                    .top_1p5()
                                    .right_2()
                                    .custom(
                                        ButtonCustomVariant::new(window, cx)
                                            .foreground(cx.theme().icon_muted)
                                            .color(cx.theme().ghost_element_background)
                                            .hover(cx.theme().ghost_element_background)
                                            .active(cx.theme().ghost_element_background),
                                    )
                                    .on_click(
                                        move |_, window, cx| {
                                            on_cancel(&ClickEvent::default(), window, cx);
                                            on_close(&ClickEvent::default(), window, cx);
                                            window.close_modal(cx);
                                        },
                                    ),
                                )
                            })
                            .child(div().relative().w_full().flex_1().child(self.content))
                            .when(self.footer.is_some(), |this| {
                                let footer = self.footer.unwrap();

                                this.child(
                                    h_flex().p_4().gap_1p5().justify_center().children(footer(
                                        render_ok,
                                        render_cancel,
                                        window,
                                        cx,
                                    )),
                                )
                            })
                            .with_animation("slide-down", animation.clone(), move |this, delta| {
                                let y_offset = px(0.) + delta * px(30.);
                                // This is equivalent to `shadow_xl` with an extra opacity.
                                let shadow = vec![
                                    BoxShadow {
                                        color: hsla(0., 0., 0., 0.1 * delta),
                                        offset: point(px(0.), px(20.)),
                                        blur_radius: px(25.),
                                        spread_radius: px(-5.),
                                    },
                                    BoxShadow {
                                        color: hsla(0., 0., 0., 0.1 * delta),
                                        offset: point(px(0.), px(8.)),
                                        blur_radius: px(10.),
                                        spread_radius: px(-6.),
                                    },
                                ];
                                this.top(y + y_offset).shadow(shadow)
                            }),
                    )
                    .with_animation("fade-in", animation, move |this, delta| this.opacity(delta)),
            )
    }
}
