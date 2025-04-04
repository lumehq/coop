use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder, px, App, ClickEvent, Img, InteractiveElement, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement, Styled as _, Window,
};
use ui::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    Collapsible, Icon, IconName, StyledExt,
};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Parent {
    icon: Option<Icon>,
    active_icon: Option<Icon>,
    label: SharedString,
    items: Vec<Folder>,
    collapsed: bool,
    handler: Handler,
}

impl Parent {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            active_icon: None,
            items: Vec::new(),
            collapsed: false,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn active_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.active_icon = Some(icon.into());
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn child(mut self, child: impl Into<Folder>) -> Self {
        self.items.push(child.into());
        self
    }

    #[allow(dead_code)]
    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<Folder>>) -> Self {
        self.items = children.into_iter().map(Into::into).collect();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl Collapsible for Parent {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl RenderOnce for Parent {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(self.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_6()
                    .rounded(px(cx.theme().radius))
                    .text_xs()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .font_semibold()
                    .when_some(self.icon, |this, icon| {
                        this.map(|this| {
                            if self.collapsed {
                                this.child(icon.size_4())
                            } else {
                                this.when_some(self.active_icon, |this, icon| {
                                    this.child(icon.size_4())
                                })
                            }
                        })
                    })
                    .child(self.label.clone())
                    .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
                    .on_click(move |ev, window, cx| handler(ev, window, cx)),
            )
            .when(!self.collapsed, |this| {
                this.child(div().flex().flex_col().gap_1().pl_3().children(self.items))
            })
    }
}

#[derive(IntoElement)]
pub struct Folder {
    icon: Option<Icon>,
    active_icon: Option<Icon>,
    label: SharedString,
    items: Vec<FolderItem>,
    collapsed: bool,
    handler: Handler,
}

impl Folder {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            active_icon: None,
            items: Vec::new(),
            collapsed: false,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn active_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.active_icon = Some(icon.into());
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl Into<FolderItem>>) -> Self {
        self.items = children.into_iter().map(Into::into).collect();
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl Collapsible for Folder {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl RenderOnce for Folder {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(self.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_6()
                    .rounded(px(cx.theme().radius))
                    .text_xs()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .font_semibold()
                    .when_some(self.icon, |this, icon| {
                        this.map(|this| {
                            if self.collapsed {
                                this.child(icon.size_4())
                            } else {
                                this.when_some(self.active_icon, |this, icon| {
                                    this.child(icon.size_4())
                                })
                            }
                        })
                    })
                    .child(self.label.clone())
                    .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::THREE)))
                    .on_click(move |ev, window, cx| handler(ev, window, cx)),
            )
            .when(!self.collapsed, |this| {
                this.child(div().flex().flex_col().gap_1().pl_6().children(self.items))
            })
    }
}

#[derive(IntoElement)]
pub struct FolderItem {
    img: Option<Img>,
    label: SharedString,
    sub_label: SharedString,
    handler: Handler,
}

impl FolderItem {
    pub fn new(label: impl Into<SharedString>, sub_label: impl Into<SharedString>) -> Self {
        Self {
            img: None,
            label: label.into(),
            sub_label: sub_label.into(),
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn img(mut self, img: Option<Img>) -> Self {
        self.img = img;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.handler = Rc::new(handler);
        self
    }
}

impl RenderOnce for FolderItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let handler = self.handler.clone();

        div()
            .id(self.label.clone())
            .h_6()
            .px_2()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .rounded(px(cx.theme().radius))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .truncate()
                    .font_medium()
                    .map(|this| {
                        if let Some(img) = self.img {
                            this.child(img.size_4().flex_shrink_0())
                        } else {
                            this.child(
                                div()
                                    .flex()
                                    .justify_center()
                                    .items_center()
                                    .size_4()
                                    .rounded_full()
                                    .bg(cx.theme().accent.step(cx, ColorScaleStep::THREE))
                                    .child(Icon::new(IconName::GroupFill).size_2().text_color(
                                        cx.theme().accent.step(cx, ColorScaleStep::TWELVE),
                                    )),
                            )
                        }
                    })
                    .child(self.label.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(cx.theme().base.step(cx, ColorScaleStep::ELEVEN))
                    .child(self.sub_label.clone()),
            )
            .hover(|this| this.bg(cx.theme().base.step(cx, ColorScaleStep::FOUR)))
            .on_click(move |ev, window, cx| handler(ev, window, cx))
    }
}
