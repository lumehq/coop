use crate::{
    theme::{scale::ColorScaleStep, ActiveTheme},
    Sizable, Size,
};
use gpui::{
    prelude::FluentBuilder as _, svg, AnyElement, App, AppContext, Entity, Hsla, IntoElement,
    Radians, Render, RenderOnce, SharedString, StyleRefinement, Styled, Svg, Transformation,
    Window,
};

#[derive(IntoElement, Clone)]
pub enum IconName {
    AddressBook,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowUpCircle,
    Bell,
    CaretUp,
    CaretDown,
    CaretDownFill,
    CaretRight,
    Check,
    CheckCircle,
    CheckCircleFill,
    Close,
    CloseCircle,
    CloseCircleFill,
    Copy,
    Ellipsis,
    Eye,
    EyeOff,
    Folder,
    FolderFill,
    Inbox,
    Info,
    Loader,
    LoaderCircle,
    MailboxFill,
    Maximize,
    Menu,
    Minimize,
    Moon,
    Palette,
    PanelBottom,
    PanelBottomOpen,
    PanelLeft,
    PanelLeftClose,
    PanelLeftOpen,
    PanelRight,
    PanelRightClose,
    PanelRightOpen,
    Plus,
    PlusCircleFill,
    Relays,
    ResizeCorner,
    Search,
    Settings,
    SortAscending,
    SortDescending,
    Sun,
    ThumbsDown,
    ThumbsUp,
    TriangleAlert,
    Upload,
    UsersThreeFill,
    WindowClose,
    WindowMaximize,
    WindowMinimize,
    WindowRestore,
}

impl IconName {
    pub fn path(self) -> SharedString {
        match self {
            Self::AddressBook => "icons/address-book.svg",
            Self::ArrowDown => "icons/arrow-down.svg",
            Self::ArrowLeft => "icons/arrow-left.svg",
            Self::ArrowRight => "icons/arrow-right.svg",
            Self::ArrowUp => "icons/arrow-up.svg",
            Self::ArrowUpCircle => "icons/arrow-up-circle.svg",
            Self::Bell => "icons/bell.svg",
            Self::CaretRight => "icons/caret-right.svg",
            Self::CaretUp => "icons/caret-up.svg",
            Self::CaretDown => "icons/caret-down.svg",
            Self::CaretDownFill => "icons/caret-down-fill.svg",
            Self::Check => "icons/check.svg",
            Self::CheckCircle => "icons/check-circle.svg",
            Self::CheckCircleFill => "icons/check-circle-fill.svg",
            Self::Close => "icons/close.svg",
            Self::CloseCircle => "icons/close-circle.svg",
            Self::CloseCircleFill => "icons/close-circle-fill.svg",
            Self::Copy => "icons/copy.svg",
            Self::Ellipsis => "icons/ellipsis.svg",
            Self::Eye => "icons/eye.svg",
            Self::EyeOff => "icons/eye-off.svg",
            Self::Folder => "icons/folder.svg",
            Self::FolderFill => "icons/folder-fill.svg",
            Self::Inbox => "icons/inbox.svg",
            Self::Info => "icons/info.svg",
            Self::Loader => "icons/loader.svg",
            Self::LoaderCircle => "icons/loader-circle.svg",
            Self::MailboxFill => "icons/mailbox-fill.svg",
            Self::Maximize => "icons/maximize.svg",
            Self::Menu => "icons/menu.svg",
            Self::Minimize => "icons/minimize.svg",
            Self::Moon => "icons/moon.svg",
            Self::Palette => "icons/palette.svg",
            Self::PanelBottom => "icons/panel-bottom.svg",
            Self::PanelBottomOpen => "icons/panel-bottom-open.svg",
            Self::PanelLeft => "icons/panel-left.svg",
            Self::PanelLeftClose => "icons/panel-left-close.svg",
            Self::PanelLeftOpen => "icons/panel-left-open.svg",
            Self::PanelRight => "icons/panel-right.svg",
            Self::PanelRightClose => "icons/panel-right-close.svg",
            Self::PanelRightOpen => "icons/panel-right-open.svg",
            Self::Plus => "icons/plus.svg",
            Self::PlusCircleFill => "icons/plus-circle-fill.svg",
            Self::Relays => "icons/relays.svg",
            Self::ResizeCorner => "icons/resize-corner.svg",
            Self::Search => "icons/search.svg",
            Self::Settings => "icons/settings.svg",
            Self::SortAscending => "icons/sort-ascending.svg",
            Self::SortDescending => "icons/sort-descending.svg",
            Self::Sun => "icons/sun.svg",
            Self::ThumbsDown => "icons/thumbs-down.svg",
            Self::ThumbsUp => "icons/thumbs-up.svg",
            Self::TriangleAlert => "icons/triangle-alert.svg",
            Self::Upload => "icons/upload.svg",
            Self::UsersThreeFill => "icons/users-three-fill.svg",
            Self::WindowClose => "icons/window-close.svg",
            Self::WindowMaximize => "icons/window-maximize.svg",
            Self::WindowMinimize => "icons/window-minimize.svg",
            Self::WindowRestore => "icons/window-restore.svg",
        }
        .into()
    }

    /// Return the icon as a Entity<Icon>
    pub fn view(self, window: &mut Window, cx: &mut App) -> Entity<Icon> {
        Icon::build(self).view(window, cx)
    }
}

impl From<IconName> for Icon {
    fn from(val: IconName) -> Self {
        Icon::build(val)
    }
}

impl From<IconName> for AnyElement {
    fn from(val: IconName) -> Self {
        Icon::build(val).into_any_element()
    }
}

impl RenderOnce for IconName {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::build(self)
    }
}

#[derive(IntoElement)]
pub struct Icon {
    base: Svg,
    path: SharedString,
    text_color: Option<Hsla>,
    size: Option<Size>,
    rotation: Option<Radians>,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            base: svg().flex_none().size_4(),
            path: "".into(),
            text_color: None,
            size: None,
            rotation: None,
        }
    }
}

impl Clone for Icon {
    fn clone(&self) -> Self {
        let mut this = Self::default().path(self.path.clone());
        if let Some(size) = self.size {
            this = this.with_size(size);
        }
        this
    }
}

pub trait IconNamed {
    fn path(&self) -> SharedString;
}

impl Icon {
    pub fn new(icon: impl Into<Icon>) -> Self {
        icon.into()
    }

    fn build(name: IconName) -> Self {
        Self::default().path(name.path())
    }

    /// Set the icon path of the Assets bundle
    ///
    /// For example: `icons/foo.svg`
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = path.into();
        self
    }

    /// Create a new view for the icon
    pub fn view(self, _window: &mut Window, cx: &mut App) -> Entity<Icon> {
        cx.new(|_| self)
    }

    pub fn transform(mut self, transformation: gpui::Transformation) -> Self {
        self.base = self.base.with_transformation(transformation);
        self
    }

    pub fn empty() -> Self {
        Self::default()
    }

    /// Rotate the icon by the given angle
    pub fn rotate(mut self, radians: impl Into<Radians>) -> Self {
        self.base = self
            .base
            .with_transformation(Transformation::rotate(radians));
        self
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }

    fn text_color(mut self, color: impl Into<Hsla>) -> Self {
        self.text_color = Some(color.into());
        self
    }
}

impl Sizable for Icon {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let text_color = self.text_color.unwrap_or_else(|| window.text_style().color);

        self.base
            .text_color(text_color)
            .when_some(self.size, |this, size| match size {
                Size::Size(px) => this.size(px),
                Size::XSmall => this.size_3(),
                Size::Small => this.size_4(),
                Size::Medium => this.size_5(),
                Size::Large => this.size_6(),
            })
            .path(self.path)
    }
}

impl From<Icon> for AnyElement {
    fn from(val: Icon) -> Self {
        val.into_any_element()
    }
}

impl Render for Icon {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let text_color = self
            .text_color
            .unwrap_or_else(|| cx.theme().base.step(cx, ColorScaleStep::ELEVEN));

        svg()
            .flex_none()
            .text_color(text_color)
            .when_some(self.size, |this, size| match size {
                Size::Size(px) => this.size(px),
                Size::XSmall => this.size_3(),
                Size::Small => this.size_4(),
                Size::Medium => this.size_5(),
                Size::Large => this.size_6(),
            })
            .path(self.path.clone())
            .when_some(self.rotation, |this, rotation| {
                this.with_transformation(Transformation::rotate(rotation))
            })
    }
}
