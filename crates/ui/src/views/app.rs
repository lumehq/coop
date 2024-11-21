use components::theme::ActiveTheme;
use gpui::*;

use crate::state::AppState;

use super::{chatspace::ChatSpaceView, setup::SetupView};

pub struct AppView {
    onboarding: Model<Option<AnyView>>, // TODO: create onboarding view
    setup: View<SetupView>,
    chat_space: View<ChatSpaceView>,
}

impl AppView {
    pub fn new(cx: &mut ViewContext<'_, Self>) -> AppView {
        // Onboarding model
        let onboarding = cx.new_model(|_| None);
        // Setup view
        let setup = cx.new_view(SetupView::new);
        // Chat Space view
        let chat_space = cx.new_view(ChatSpaceView::new);

        cx.foreground_executor()
            .spawn(async move {
                // TODO: create onboarding view for the first time open app
            })
            .detach();

        AppView {
            onboarding,
            setup,
            chat_space,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let mut content = div().size_full().flex().items_center().justify_center();

        if cx.global::<AppState>().accounts.is_empty() {
            content = content.child(self.setup.clone())
        } else {
            content = content.child(self.chat_space.clone())
        }

        if let Some(onboarding) = self.onboarding.read(cx).as_ref() {
            content = content.child(onboarding.clone())
        }

        div()
            .bg(cx.theme().background)
            .text_color(rgb(0xFFFFFF))
            .size_full()
            .child(content)
    }
}
