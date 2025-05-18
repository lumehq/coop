use std::rc::Rc;

use gpui::{
    div, percentage, prelude::FluentBuilder, App, ClickEvent, Div, Img, InteractiveElement,
    IntoElement, ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use theme::ActiveTheme;
use ui::{tooltip::Tooltip, Collapsible, Icon, IconName, Sizable, StyledExt};

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Parent {
    base: Div,
    icon: Option<Icon>,
    tooltip: Option<SharedString>,
    label: SharedString,
    items: Vec<Folder>,
    collapsed: bool,
    handler: Handler,
}

impl Parent {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            base: div().flex().flex_col().gap_2(),
            label: label.into(),
            icon: None,
            tooltip: None,
            items: Vec::new(),
            collapsed: false,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
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

        self.base
            .child(
                div()
                    .id(self.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_8()
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .font_medium()
                    .child(
                        Icon::new(IconName::CaretDown)
                            .xsmall()
                            .when(self.collapsed, |this| this.rotate(percentage(270. / 360.))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when_some(self.icon, |this, icon| this.child(icon.small()))
                            .child(self.label.clone()),
                    )
                    .when_some(self.tooltip.clone(), |this, tooltip| {
                        this.tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone(), window, cx).into()
                        })
                    })
                    .hover(|this| this.bg(cx.theme().elevated_surface_background))
                    .on_click(move |ev, window, cx| handler(ev, window, cx)),
            )
            .when(!self.collapsed, |this| {
                this.child(div().flex().flex_col().gap_2().pl_3().children(self.items))
            })
    }
}

#[derive(IntoElement)]
pub struct Folder {
    base: Div,
    icon: Option<Icon>,
    tooltip: Option<SharedString>,
    label: SharedString,
    items: Vec<FolderItem>,
    collapsed: bool,
    handler: Handler,
}

impl Folder {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            base: div().flex().flex_col().gap_2(),
            label: label.into(),
            icon: None,
            tooltip: None,
            items: Vec::new(),
            collapsed: false,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
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

        self.base
            .child(
                div()
                    .id(self.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .h_8()
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .text_color(cx.theme().text_muted)
                    .font_medium()
                    .child(
                        Icon::new(IconName::CaretDown)
                            .xsmall()
                            .when(self.collapsed, |this| this.rotate(percentage(270. / 360.))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when_some(self.icon, |this, icon| this.child(icon.small()))
                            .child(self.label.clone()),
                    )
                    .when_some(self.tooltip.clone(), |this, tooltip| {
                        this.tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone(), window, cx).into()
                        })
                    })
                    .hover(|this| this.bg(cx.theme().elevated_surface_background))
                    .on_click(move |ev, window, cx| handler(ev, window, cx)),
            )
            .when(!self.collapsed, |this| {
                this.child(div().flex().flex_col().gap_1().pl_6().children(self.items))
            })
    }
}

#[derive(IntoElement)]
pub struct FolderItem {
    ix: usize,
    base: Div,
    img: Option<Img>,
    label: Option<SharedString>,
    description: Option<SharedString>,
    handler: Handler,
}

impl FolderItem {
    pub fn new(ix: usize) -> Self {
        Self {
            ix,
            base: div().h_8().w_full().px_2(),
            img: None,
            label: None,
            description: None,
            handler: Rc::new(|_, _, _| {}),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn img(mut self, img: Img) -> Self {
        self.img = Some(img);
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

        self.base
            .id(self.ix)
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .rounded(cx.theme().radius)
            .child(div().size_6().flex_none().map(|this| {
                if let Some(img) = self.img {
                    this.child(img.size_6().flex_none())
                } else {
                    this.child(
                        div()
                            .size_6()
                            .flex_none()
                            .flex()
                            .justify_center()
                            .items_center()
                            .rounded_full()
                            .bg(cx.theme().element_background),
                    )
                }
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when_some(self.label, |this, label| {
                        this.child(div().truncate().text_ellipsis().font_medium().child(label))
                    })
                    .when_some(self.description, |this, description| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().text_placeholder)
                                .child(description),
                        )
                    }),
            )
            .hover(|this| this.bg(cx.theme().elevated_surface_background))
            .on_click(move |ev, window, cx| handler(ev, window, cx))
    }
}
