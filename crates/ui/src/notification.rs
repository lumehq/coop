use std::any::TypeId;
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Animation, AnimationExt, AnyElement, App, AppContext, ClickEvent, Context,
    DismissEvent, ElementId, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement, StyleRefinement, Styled,
    Subscription, Window,
};
use smol::Timer;
use theme::ActiveTheme;

use crate::animation::cubic_bezier;
use crate::button::{Button, ButtonVariants as _};
use crate::{h_flex, v_flex, Icon, IconName, Sizable as _, StyledExt};

#[derive(Debug, Clone, Copy, Default)]
pub enum NotificationType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationType {
    fn icon(&self, cx: &App) -> Icon {
        match self {
            Self::Info => Icon::new(IconName::Info).text_color(cx.theme().element_foreground),
            Self::Success => Icon::new(IconName::Info).text_color(cx.theme().secondary_foreground),
            Self::Warning => Icon::new(IconName::Warning).text_color(cx.theme().warning_foreground),
            Self::Error => Icon::new(IconName::Warning).text_color(cx.theme().danger_foreground),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub(crate) enum NotificationId {
    Id(TypeId),
    IdAndElementId(TypeId, ElementId),
}

impl From<TypeId> for NotificationId {
    fn from(type_id: TypeId) -> Self {
        Self::Id(type_id)
    }
}

impl From<(TypeId, ElementId)> for NotificationId {
    fn from((type_id, id): (TypeId, ElementId)) -> Self {
        Self::IdAndElementId(type_id, id)
    }
}

/// A notification element.
pub struct Notification {
    /// The id is used make the notification unique.
    /// Then you push a notification with the same id, the previous notification will be replaced.
    ///
    /// None means the notification will be added to the end of the list.
    id: NotificationId,
    style: StyleRefinement,
    type_: Option<NotificationType>,
    title: Option<SharedString>,
    message: Option<SharedString>,
    icon: Option<Icon>,
    autohide: bool,
    #[allow(clippy::type_complexity)]
    action_builder: Option<Rc<dyn Fn(&mut Window, &mut Context<Self>) -> Button>>,
    #[allow(clippy::type_complexity)]
    content_builder: Option<Rc<dyn Fn(&mut Window, &mut Context<Self>) -> AnyElement>>,
    #[allow(clippy::type_complexity)]
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
    closing: bool,
}

impl From<String> for Notification {
    fn from(s: String) -> Self {
        Self::new().message(s)
    }
}

impl From<Cow<'static, str>> for Notification {
    fn from(s: Cow<'static, str>) -> Self {
        Self::new().message(s)
    }
}

impl From<SharedString> for Notification {
    fn from(s: SharedString) -> Self {
        Self::new().message(s)
    }
}

impl From<&'static str> for Notification {
    fn from(s: &'static str) -> Self {
        Self::new().message(s)
    }
}

impl From<(NotificationType, &'static str)> for Notification {
    fn from((type_, content): (NotificationType, &'static str)) -> Self {
        Self::new().message(content).with_type(type_)
    }
}

impl From<(NotificationType, SharedString)> for Notification {
    fn from((type_, content): (NotificationType, SharedString)) -> Self {
        Self::new().message(content).with_type(type_)
    }
}

struct DefaultIdType;

impl Notification {
    /// Create a new notification with the given content.
    ///
    /// default width is 320px.
    pub fn new() -> Self {
        let id: SharedString = uuid::Uuid::new_v4().to_string().into();
        let id = (TypeId::of::<DefaultIdType>(), id.into());

        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: None,
            message: None,
            type_: None,
            icon: None,
            autohide: true,
            action_builder: None,
            content_builder: None,
            on_click: None,
            closing: false,
        }
    }

    pub fn message(mut self, message: impl Into<SharedString>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn info(message: impl Into<SharedString>) -> Self {
        Self::new()
            .message(message)
            .with_type(NotificationType::Info)
    }

    pub fn success(message: impl Into<SharedString>) -> Self {
        Self::new()
            .message(message)
            .with_type(NotificationType::Success)
    }

    pub fn warning(message: impl Into<SharedString>) -> Self {
        Self::new()
            .message(message)
            .with_type(NotificationType::Warning)
    }

    pub fn error(message: impl Into<SharedString>) -> Self {
        Self::new()
            .message(message)
            .with_type(NotificationType::Error)
    }

    /// Set the type for unique identification of the notification.
    ///
    /// ```rs
    /// struct MyNotificationKind;
    /// let notification = Notification::new("Hello").id::<MyNotificationKind>();
    /// ```
    pub fn id<T: Sized + 'static>(mut self) -> Self {
        self.id = TypeId::of::<T>().into();
        self
    }

    /// Set the type and id of the notification, used to uniquely identify the notification.
    pub fn custom_id(mut self, key: impl Into<ElementId>) -> Self {
        self.id = (TypeId::of::<DefaultIdType>(), key.into()).into();
        self
    }

    /// Set the title of the notification, default is None.
    ///
    /// If title is None, the notification will not have a title.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the icon of the notification.
    ///
    /// If icon is None, the notification will use the default icon of the type.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the type of the notification, default is NotificationType::Info.
    pub fn with_type(mut self, type_: NotificationType) -> Self {
        self.type_ = Some(type_);
        self
    }

    /// Set the auto hide of the notification, default is true.
    pub fn autohide(mut self, autohide: bool) -> Self {
        self.autohide = autohide;
        self
    }

    /// Set the click callback of the notification.
    pub fn on_click(
        mut self,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }

    /// Set the action button of the notification.
    pub fn action<F>(mut self, action: F) -> Self
    where
        F: Fn(&mut Window, &mut Context<Self>) -> Button + 'static,
    {
        self.action_builder = Some(Rc::new(action));
        self
    }

    /// Dismiss the notification.
    pub fn dismiss(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.closing = true;
        cx.notify();

        // Dismiss the notification after 0.15s to show the animation.
        cx.spawn(async move |view, cx| {
            Timer::after(Duration::from_secs_f32(0.15)).await;
            cx.update(|cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |view, cx| {
                        view.closing = false;
                        cx.emit(DismissEvent);
                    });
                }
            })
        })
        .detach()
    }

    /// Set the content of the notification.
    pub fn content(
        mut self,
        content: impl Fn(&mut Window, &mut Context<Self>) -> AnyElement + 'static,
    ) -> Self {
        self.content_builder = Some(Rc::new(content));
        self
    }
}

impl Default for Notification {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<DismissEvent> for Notification {}

impl FluentBuilder for Notification {}

impl Styled for Notification {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Render for Notification {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let closing = self.closing;
        let icon = match self.type_ {
            None => self.icon.clone(),
            Some(type_) => Some(type_.icon(cx)),
        };

        h_flex()
            .id("notification")
            .refine_style(&self.style)
            .group("")
            .occlude()
            .relative()
            .w_96()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().surface_background)
            .rounded(cx.theme().radius * 1.6)
            .shadow_md()
            .p_2()
            .gap_3()
            .justify_start()
            .items_start()
            .when_some(icon, |this, icon| {
                this.child(div().flex_shrink_0().pt_1().child(icon))
            })
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .overflow_hidden()
                    .when_some(self.title.clone(), |this, title| {
                        this.child(div().text_sm().font_semibold().child(title))
                    })
                    .when_some(self.message.clone(), |this, message| {
                        this.child(div().text_sm().child(message))
                    })
                    .when_some(self.content_builder.clone(), |this, child_builder| {
                        this.child(child_builder(window, cx))
                    })
                    .when_some(self.action_builder.clone(), |this, action_builder| {
                        this.child(action_builder(window, cx).small().w_full().my_2())
                    }),
            )
            .child(
                div()
                    .absolute()
                    .top_2p5()
                    .right_2p5()
                    .invisible()
                    .group_hover("", |this| this.visible())
                    .child(
                        Button::new("close")
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss(window, cx);
                            })),
                    ),
            )
            .when_some(self.on_click.clone(), |this, on_click| {
                this.on_click(cx.listener(move |view, event, window, cx| {
                    view.dismiss(window, cx);
                    on_click(event, window, cx);
                }))
            })
            .with_animation(
                ElementId::NamedInteger("slide-down".into(), closing as u64),
                Animation::new(Duration::from_secs_f64(0.25))
                    .with_easing(cubic_bezier(0.4, 0., 0.2, 1.)),
                move |this, delta| {
                    if closing {
                        let x_offset = px(0.) + delta * px(45.);
                        let opacity = 1. - delta;
                        this.left(px(0.) + x_offset)
                            .shadow_none()
                            .opacity(opacity)
                            .when(opacity < 0.85, |this| this.shadow_none())
                    } else {
                        let y_offset = px(-45.) + delta * px(45.);
                        let opacity = delta;
                        this.top(px(0.) + y_offset)
                            .opacity(opacity)
                            .when(opacity < 0.85, |this| this.shadow_none())
                    }
                },
            )
    }
}

/// A list of notifications.
pub struct NotificationList {
    /// Notifications that will be auto hidden.
    pub(crate) notifications: VecDeque<Entity<Notification>>,
    expanded: bool,
    _subscriptions: HashMap<NotificationId, Subscription>,
}

impl NotificationList {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            notifications: VecDeque::new(),
            expanded: false,
            _subscriptions: HashMap::new(),
        }
    }

    pub fn push<T>(&mut self, notification: T, window: &mut Window, cx: &mut Context<Self>)
    where
        T: Into<Notification>,
    {
        let notification = notification.into();
        let id = notification.id.clone();
        let autohide = notification.autohide;

        // Remove the notification by id, for keep unique.
        self.notifications.retain(|note| note.read(cx).id != id);

        let notification = cx.new(|_| notification);

        self._subscriptions.insert(
            id.clone(),
            cx.subscribe(&notification, move |view, _, _: &DismissEvent, cx| {
                view.notifications.retain(|note| id != note.read(cx).id);
                view._subscriptions.remove(&id);
            }),
        );

        self.notifications.push_back(notification.clone());

        if autohide {
            // Sleep for 5 seconds to autohide the notification
            cx.spawn_in(window, async move |_, cx| {
                Timer::after(Duration::from_secs(5)).await;

                if let Err(error) =
                    notification.update_in(cx, |note, window, cx| note.dismiss(window, cx))
                {
                    log::error!("Failed to auto hide notification: {error}");
                }
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn close<T>(&mut self, key: T, window: &mut Window, cx: &mut Context<Self>)
    where
        T: Into<ElementId>,
    {
        let id = (TypeId::of::<DefaultIdType>(), key.into()).into();

        if let Some(n) = self.notifications.iter().find(|n| n.read(cx).id == id) {
            n.update(cx, |note, cx| {
                note.dismiss(window, cx);
            });
        }

        cx.notify();
    }

    pub fn clear(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notifications.clear();
        cx.notify();
    }

    pub fn notifications(&self) -> Vec<Entity<Notification>> {
        self.notifications.iter().cloned().collect()
    }
}

impl Render for NotificationList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = window.viewport_size();
        let items = self.notifications.iter().rev().take(10).rev().cloned();

        div()
            .id("notification-wrapper")
            .absolute()
            .top_4()
            .right_4()
            .child(
                v_flex()
                    .id("notification-list")
                    .h(size.height - px(8.))
                    .gap_3()
                    .children(items)
                    .on_hover(cx.listener(|view, hovered, _, cx| {
                        view.expanded = *hovered;
                        cx.notify()
                    })),
            )
    }
}
