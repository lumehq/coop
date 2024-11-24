use components::theme::ActiveTheme;
use gpui::*;

use super::{chat_space::ChatSpace, onboarding::Onboarding};
use crate::state::AppState;

pub struct AppView {
    onboarding: View<Onboarding>,
    chat_space: View<ChatSpace>,
}

impl AppView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> AppView {
        // Onboarding
        let onboarding = cx.new_view(Onboarding::new);
        // Chat Space
        let chat_space = cx.new_view(ChatSpace::new);

        AppView {
            onboarding,
            chat_space,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div().size_full().flex().items_center().justify_center();

        if cx.global::<AppState>().signer.is_none() {
            content = content.child(self.onboarding.clone())
        } else {
            content = content.child(self.chat_space.clone())
        }

        div()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .size_full()
            .child(content)
    }
}
