use components::{
    button::Button,
    dock::{DockItemState, Panel, PanelEvent, TitleStyle},
    h_flex,
    popup_menu::PopupMenu,
    theme::ActiveTheme,
    v_flex,
};
use gpui::*;
use prelude::FluentBuilder;

pub mod welcome;

actions!(block, [PanelInfo]);

pub fn section(title: impl IntoElement, cx: &WindowContext) -> Div {
    h_flex()
        .items_center()
        .gap_4()
        .p_4()
        .w_full()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .flex_wrap()
        .justify_around()
        .child(div().flex_none().w_full().child(title))
}

pub struct BlockContainer {
    focus_handle: FocusHandle,
    name: SharedString,
    title_bg: Option<Hsla>,
    description: SharedString,
    width: Option<Pixels>,
    height: Option<Pixels>,
    block: Option<AnyView>,
    closeable: bool,
    zoomable: bool,
}

#[derive(Debug)]
pub enum ContainerEvent {
    Close,
}

pub trait Block: FocusableView {
    fn klass() -> &'static str {
        std::any::type_name::<Self>().split("::").last().unwrap()
    }

    fn title() -> &'static str;

    fn description() -> &'static str {
        ""
    }

    fn closeable() -> bool {
        true
    }

    fn zoomable() -> bool {
        true
    }

    fn title_bg() -> Option<Hsla> {
        None
    }

    fn new_view(cx: &mut WindowContext) -> View<impl FocusableView>;
}

impl EventEmitter<ContainerEvent> for BlockContainer {}

impl BlockContainer {
    pub fn new(cx: &mut WindowContext) -> Self {
        let focus_handle = cx.focus_handle();

        Self {
            focus_handle,
            name: "".into(),
            title_bg: None,
            description: "".into(),
            width: None,
            height: None,
            block: None,
            closeable: true,
            zoomable: true,
        }
    }

    pub fn panel<B: Block>(cx: &mut WindowContext) -> View<Self> {
        let name = B::title();
        let description = B::description();
        let block = B::new_view(cx);
        let focus_handle = block.focus_handle(cx);

        cx.new_view(|cx| {
            let mut story = Self::new(cx).block(block.into());

            story.focus_handle = focus_handle;
            story.closeable = B::closeable();
            story.zoomable = B::zoomable();
            story.name = name.into();
            story.description = description.into();
            story.title_bg = B::title_bg();

            story
        })
    }

    pub fn width(mut self, width: gpui::Pixels) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: gpui::Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn block(mut self, block: AnyView) -> Self {
        self.block = Some(block);
        self
    }

    fn on_action_panel_info(&mut self, _: &PanelInfo, _cx: &mut ViewContext<Self>) {
        // struct Info;
        // let note = Notification::new(format!("You have clicked panel info on: {}", self.name)).id::<Info>();
        // cx.push_notification(note);
    }
}

impl Panel for BlockContainer {
    fn panel_name(&self) -> &'static str {
        "BlockContainer"
    }

    fn title(&self, _cx: &WindowContext) -> AnyElement {
        self.name.clone().into_any_element()
    }

    fn title_style(&self, cx: &WindowContext) -> Option<TitleStyle> {
        self.title_bg.map(|bg| TitleStyle {
            background: bg,
            foreground: cx.theme().foreground,
        })
    }

    fn closeable(&self, _cx: &WindowContext) -> bool {
        self.closeable
    }

    fn zoomable(&self, _cx: &WindowContext) -> bool {
        self.zoomable
    }

    fn popup_menu(&self, menu: PopupMenu, _cx: &WindowContext) -> PopupMenu {
        menu.track_focus(&self.focus_handle)
            .menu("Info", Box::new(PanelInfo))
    }

    fn toolbar_buttons(&self, _cx: &WindowContext) -> Vec<Button> {
        vec![]
    }

    fn dump(&self, _cx: &AppContext) -> DockItemState {
        DockItemState::new(self)
    }
}

impl EventEmitter<PanelEvent> for BlockContainer {}

impl FocusableView for BlockContainer {
    fn focus_handle(&self, _: &AppContext) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BlockContainer {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_panel_info))
            .when_some(self.block.clone(), |this, story| {
                this.child(v_flex().size_full().child(story))
            })
    }
}
