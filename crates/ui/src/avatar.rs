use gpui::{
    div, img, prelude::FluentBuilder, px, rems, AbsoluteLength, App, Hsla, ImageSource, Img,
    IntoElement, ParentElement, RenderOnce, Styled, StyledImage, Window,
};
use theme::ActiveTheme;

/// An element that renders a user avatar with customizable appearance options.
///
/// # Examples
///
/// ```
/// use ui::{Avatar};
///
/// Avatar::new("path/to/image.png")
///     .grayscale(true)
///     .border_color(gpui::red());
/// ```
#[derive(IntoElement)]
pub struct Avatar {
    image: Img,
    size: Option<AbsoluteLength>,
    border_color: Option<Hsla>,
}

impl Avatar {
    /// Creates a new avatar element with the specified image source.
    pub fn new(src: impl Into<ImageSource>) -> Self {
        Avatar {
            image: img(src),
            size: None,
            border_color: None,
        }
    }

    /// Applies a grayscale filter to the avatar image.
    ///
    /// # Examples
    ///
    /// ```
    /// use ui::{Avatar, AvatarShape};
    ///
    /// let avatar = Avatar::new("path/to/image.png").grayscale(true);
    /// ```
    pub fn grayscale(mut self, grayscale: bool) -> Self {
        self.image = self.image.grayscale(grayscale);
        self
    }

    /// Sets the border color of the avatar.
    ///
    /// This might be used to match the border to the background color of
    /// the parent element to create the illusion of cropping another
    /// shape underneath (for example in face piles.)
    pub fn border_color(mut self, color: impl Into<Hsla>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Size overrides the avatar size. By default they are 1rem.
    pub fn size<L: Into<AbsoluteLength>>(mut self, size: impl Into<Option<L>>) -> Self {
        self.size = size.into().map(Into::into);
        self
    }
}

impl RenderOnce for Avatar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let border_width = if self.border_color.is_some() {
            px(2.)
        } else {
            px(0.)
        };

        let image_size = self.size.unwrap_or_else(|| rems(1.).into());
        let container_size = image_size.to_pixels(window.rem_size()) + border_width * 2.;

        div()
            .flex_shrink_0()
            .size(container_size)
            .rounded_full()
            .overflow_hidden()
            .when_some(self.border_color, |this, color| {
                this.border(border_width).border_color(color)
            })
            .child(
                self.image
                    .size(image_size)
                    .rounded_full()
                    .object_fit(gpui::ObjectFit::Fill)
                    .bg(cx.theme().ghost_element_background)
                    .with_fallback(move || {
                        img("brand/avatar.png")
                            .size(image_size)
                            .rounded_full()
                            .into_any_element()
                    }),
            )
    }
}
