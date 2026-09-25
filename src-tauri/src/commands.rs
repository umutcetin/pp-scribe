use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::audio_toolkit::audio::list_output_devices;
use crate::audio_toolkit::list_input_devices;
use crate::meeting::manager::ActiveSnapshot;
use crate::meeting::storage::{self, MeetingDetail, MeetingMeta};
use crate::meeting::MeetingManager;
use crate::settings::{self, AppSettings};
use crate::stt::models::ModelInfo;

type Manager<'a> = State<'a, Arc<MeetingManager>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    settings::get_settings(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, manager: Manager, settings: AppSettings) {
    let before = settings::get_settings(&app);
    settings::write_settings(&app, &settings);
    if before.live_model != settings.live_model {
        manager.on_settings_changed();
    }
}

#[derive(Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct AudioDevices {
    pub inputs: Vec<DeviceInfo>,
    pub default_output: Option<String>,
}

#[tauri::command]
pub fn list_audio_devices() -> Result<AudioDevices, String> {
    let inputs = list_input_devices()
        .map_err(err)?
        .into_iter()
        .map(|d| DeviceInfo { name: d.name, is_default: d.is_default })
        .collect();
    let default_output = list_output_devices()
        .ok()
        .and_then(|l| l.into_iter().find(|d| d.is_default).map(|d| d.name));
    Ok(AudioDevices { inputs, default_output })
}

#[tauri::command]
pub fn list_models(manager: Manager) -> Vec<ModelInfo> {
    manager.models.list()
}

#[tauri::command]
pub async fn download_model(manager: State<'_, Arc<MeetingManager>>, model_id: String) -> Result<(), String> {
    let models = manager.models.clone();
    models.download(&model_id).await.map_err(err)
}

#[tauri::command]
pub fn start_meeting(manager: Manager, title: Option<String>) -> Result<MeetingMeta, String> {
    manager.start(title).map_err(err)
}

#[tauri::command]
pub async fn stop_meeting(manager: State<'_, Arc<MeetingManager>>) -> Result<MeetingMeta, String> {
    let m = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || m.stop())
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub fn get_active_meeting(manager: Manager) -> Option<ActiveSnapshot> {
    manager.snapshot()
}

#[tauri::command]
pub fn list_meetings(app: AppHandle) -> Result<Vec<MeetingMeta>, String> {
    storage::list(&app).map_err(err)
}

#[tauri::command]
pub fn get_meeting(app: AppHandle, id: String) -> Result<MeetingDetail, String> {
    storage::load_detail(&app, &id).map_err(err)
}

#[tauri::command]
pub fn rename_meeting(app: AppHandle, manager: Manager, id: String, title: String) -> Result<(), String> {
    if manager.active_id().as_deref() == Some(id.as_str()) {
        return manager.rename_active(&title).map_err(err);
    }
    let dir = storage::meeting_dir(&app, &id).map_err(err)?;
    let mut meta = storage::read_meta(&dir).map_err(err)?;
    meta.title = title.trim().to_string();
    storage::write_meta(&dir, &meta).map_err(err)?;
    storage::export_markdown(&app, &id).map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn delete_meeting(app: AppHandle, manager: Manager, id: String) -> Result<(), String> {
    if manager.active_id().as_deref() == Some(id.as_str()) {
        return Err("Stop the meeting before deleting it".into());
    }
    storage::trash(&app, &id).map_err(err)
}

#[tauri::command]
pub fn refine_meeting(manager: Manager, id: String) -> Result<(), String> {
    manager.refine(&id).map_err(err)
}

#[tauri::command]
pub fn is_refining(manager: Manager) -> bool {
    manager.is_refining()
}

#[tauri::command]
pub fn export_meeting(app: AppHandle, id: String) -> Result<String, String> {
    storage::export_markdown(&app, &id)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}

#[tauri::command]
pub fn meetings_root(app: AppHandle) -> Result<String, String> {
    storage::root(&app)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}
