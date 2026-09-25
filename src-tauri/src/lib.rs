pub mod audio_toolkit;
pub mod commands;
pub mod meeting;
pub mod settings;
pub mod stt;

use std::sync::Arc;

use meeting::MeetingManager;
use stt::ModelManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            let models = Arc::new(ModelManager::new(app.handle())?);
            let manager = Arc::new(MeetingManager::new(app.handle(), models));
            manager.preload_live_model();
            app.manage(manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_audio_devices,
            commands::list_models,
            commands::download_model,
            commands::start_meeting,
            commands::stop_meeting,
            commands::get_active_meeting,
            commands::list_meetings,
            commands::get_meeting,
            commands::rename_meeting,
            commands::delete_meeting,
            commands::refine_meeting,
            commands::is_refining,
            commands::export_meeting,
            commands::meetings_root,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
