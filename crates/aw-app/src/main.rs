#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod art;
mod commands;
mod diag;
mod games;
mod i18n;
mod overlay;
mod popup;
mod popup_queue;
mod presets;
mod protocol;
mod settings;
mod shot;
mod sound;
mod sounds;
mod system_theme;
mod updater;
mod video;

use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--test-notification") {
                app::send_test_popup(app, None, popup::PopupKind::Unlock, false);
            } else if !args.iter().any(|a| a == "--autostart") {
                app::show_main(app);
            }
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--autostart"])
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_uri_scheme_protocol("aw", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::list_games,
            commands::game_detail,
            commands::get_settings,
            commands::save_settings,
            commands::list_presets,
            commands::preview_preset,
            commands::list_sounds,
            commands::preview_sound,
            commands::open_user_folder,
            commands::test_notification,
            commands::rescan,
            commands::diagnostics,
            commands::export_diagnostics,
            commands::open_path,
            commands::overlay_game,
            commands::hotkey_error,
            commands::toggle_overlay,
            commands::ui_locale,
            commands::supported_locales,
            commands::ffmpeg_status,
            commands::download_ffmpeg,
            commands::cancel_ffmpeg,
            commands::update_status,
            commands::check_update,
            commands::install_update,
        ])
        .setup(app::setup)
        .on_window_event(|window, event| {
            if window.label() == app::MAIN_LABEL
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event
                && let Some(state) = handle.try_state::<app::AppState>()
            {
                games::flush_all(&state);
                video::stop(handle, None);
                state.flush_logs();
            }
        }),
        Err(error) => eprintln!("achievement-watcher: {error}"),
    }
}
