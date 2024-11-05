#[cfg(target_os = "macos")]
use border::WebviewWindowExt as WebviewWindowExtAlt;
use tauri::{Manager, WebviewWindowBuilder};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_decorum::WebviewWindowExt;

pub fn setup_tray_icon(app: &tauri::AppHandle) -> tauri::Result<()> {
    let tray = app.tray_by_id("main").expect("Error: tray icon not found.");

    let menu = tauri::menu::MenuBuilder::new(app)
        .item(
            &tauri::menu::MenuItem::with_id(app, "open", "Open Coop", true, None::<&str>).unwrap(),
        )
        .item(&tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap())
        .build()
        .expect("Error: cannot create menu.");

    if tray.set_menu(Some(menu)).is_err() {
        panic!("Error: cannot set menu for tray icon.")
    }

    tray.on_menu_event(move |app, event| match event.id.0.as_str() {
        "open" => {
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or_default() {
                    let _ = window.set_focus();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                };
            } else {
                let config = app.config().app.windows.first().unwrap();
                let window = WebviewWindowBuilder::from_config(app, config)
                    .unwrap()
                    .build()
                    .unwrap();

                // Set custom decoration
                #[cfg(target_os = "windows")]
                window.create_overlay_titlebar().unwrap();

                // Set traffic light inset
                #[cfg(target_os = "macos")]
                window.set_traffic_lights_inset(12.0, 18.0).unwrap();

                // Restore native border
                #[cfg(target_os = "macos")]
                window.add_border(None);
            }
        }
        "quit" => std::process::exit(0),
        _ => {}
    });

    Ok(())
}
