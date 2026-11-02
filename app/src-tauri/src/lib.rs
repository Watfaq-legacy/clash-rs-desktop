mod commands;
mod nodes;
mod state;
mod tray;

use clash_rs_desktop_core::{AppPaths, Engine};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use crate::state::AppState;

/// Passed by the autostart entry so a login launch stays in the tray.
const AUTOSTART_FLAG: &str = "--autostart";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch just surfaces the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            let engine = Engine::load(AppPaths::from_program_data(data_dir))?;
            let auto_connect = engine.settings().settings().auto_connect;
            app.manage(AppState::new(engine));

            tray::create(app.handle())?;
            commands::spawn_health_watch(app.handle().clone());

            let from_autostart = std::env::args().any(|arg| arg == AUTOSTART_FLAG);
            if !from_autostart && let Some(window) = app.get_webview_window("main") {
                window.show()?;
            }

            let has_profile = !app.state::<AppState>().view().profiles.is_empty();
            if auto_connect && has_profile {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = commands::connect_flow(&handle).await;
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the connection alive in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_view,
            commands::connect,
            commands::disconnect,
            commands::toggle_connection,
            commands::dismiss_error,
            commands::add_subscription,
            commands::refresh_subscription,
            commands::select_profile,
            commands::remove_profile,
            commands::set_routing_mode,
            commands::set_auto_connect,
            commands::get_nodes,
            commands::select_node,
            commands::test_delays,
            commands::core_logs,
            commands::read_override,
            commands::write_override,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                restore_network(app);
            }
        });
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    quit(&app);
}

/// Disconnect (restoring the OS proxy) and exit.
pub fn quit(app: &AppHandle) {
    restore_network(app);
    app.exit(0);
}

fn restore_network(app: &AppHandle) {
    let state = app.state::<AppState>();
    // Wait briefly for an in-flight command. If it doesn't finish, exiting is
    // still safe: the job object kills the core and the proxy snapshot left
    // on disk is restored on the next launch.
    let lock = tauri::async_runtime::block_on(tokio::time::timeout(
        std::time::Duration::from_secs(3),
        state.engine.lock(),
    ));
    if let Ok(mut engine) = lock {
        let _ = engine.disconnect();
    }
}
