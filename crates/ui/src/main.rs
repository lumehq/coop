use gpui::*;
use nostr::state::get_client;

struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .bg(rgb(0xffffff))
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .child(format!("Hello, {}!", &self.text))
    }
}

#[tokio::main]
async fn main() {
    let _client = get_client().await;

    App::new().run(|cx: &mut AppContext| {
        let bounds = Bounds::centered(None, size(px(860.0), px(650.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::new_static("coop")),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |cx| {
                cx.new_view(|_cx| HelloWorld {
                    text: "coop".into(),
                })
            },
        )
        .unwrap();
    });
}
