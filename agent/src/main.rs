#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use printbridge_agent::labels::{serve, LabelSettings, Pairing, PAIRING_PORT};
use serde::Serialize;
use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, WindowEvent};

#[derive(Default)]
struct Setup {
    status: String,
    pairing: Option<Arc<Pairing>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupDetails {
    status: String,
    settings_path: String,
    endpoint: String,
    tokens: BTreeMap<String, String>,
}

#[tauri::command]
fn label_setup(state: tauri::State<'_, Arc<Mutex<Setup>>>, window: tauri::Window) -> Result<SetupDetails, String> {
    if window.label() != "main" {
        return Err("Pairing is available only in the local setup window".into());
    }
    let state = state.lock().map_err(|_| "Cannot read agent setup")?;
    Ok(SetupDetails {
        status: state.status.clone(),
        settings_path: LabelSettings::directory().map(|path| path.join("label-settings.json").display().to_string())
            .unwrap_or_else(|_| "Local application directory unavailable".into()),
        endpoint: format!("ws://127.0.0.1:{PAIRING_PORT}/labels"),
        tokens: state.pairing.as_ref().map_or_else(BTreeMap::new, |pairing| pairing.local_tokens()),
    })
}

fn show_setup(app: &tauri::AppHandle) {
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    env_logger::Builder::new().filter_level(log::LevelFilter::Off)
        .filter_module("printbridge_agent", log::LevelFilter::Info).init();
    let state = Arc::new(Mutex::new(Setup { status: "Starting…".into(), pairing: None }));
    let tray = SystemTray::new().with_menu(SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("setup", "Label setup and pairing"))
        .add_item(CustomMenuItem::new("quit", "Quit PrintBridge")));
    tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![label_setup])
        .system_tray(tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::LeftClick { .. } => show_setup(app),
            SystemTrayEvent::MenuItemClick { id, .. } if id == "setup" => show_setup(app),
            SystemTrayEvent::MenuItemClick { id, .. } if id == "quit" => app.exit(0),
            _ => {}
        })
        .on_window_event(|event| {
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                let _ = event.window().hide();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            let app_handle = app.handle();
            let state = state.clone();
            tauri::async_runtime::spawn(async move {
                let run = async {
                    let settings = Arc::new(LabelSettings::load()?);
                    let pairing = Arc::new(Pairing::new(&settings));
                    serve(settings, pairing.clone(), || {
                        if let Ok(mut ui) = state.lock() {
                            ui.status = if pairing.local_tokens().is_empty() {
                                "Listening; configure a trusted origin to enable pairing"
                            } else {
                                "Listening; select the origin you want to pair"
                            }.into();
                            ui.pairing = Some(pairing);
                        }
                        let _ = app_handle.emit_all("label-setup-changed", ());
                    }).await
                };
                if let Err(error) = run.await {
                    log::error!("Label agent startup/server error: {error:#}");
                    if let Ok(mut ui) = state.lock() {
                        ui.status = format!("{error:#}");
                        ui.pairing = None;
                    }
                    let _ = app_handle.emit_all("label-setup-changed", ());
                    show_setup(&app_handle);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error running PrintBridge");
}
