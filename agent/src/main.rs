#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod cache;
mod config;
mod download;
mod models;
mod print;
mod protocol;
mod server;

#[cfg(test)]
mod models_test;
#[cfg(test)]
mod protocol_test;

use config::Settings;
use server::{MessageHandler, WebSocketServer};
use std::sync::Arc;
use tauri::{
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    WindowEvent,
};

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("PrintBridge Agent v{} starting...", env!("CARGO_PKG_VERSION"));

    // Load settings
    let settings = match Settings::load() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            log::error!("Failed to load settings: {}", e);
            eprintln!("Failed to load settings: {}", e);
            std::process::exit(1);
        }
    };

    // Create system tray menu
    let tray_menu = create_tray_menu();
    let system_tray = SystemTray::new().with_menu(tray_menu);

    // Build Tauri application
    tauri::Builder::default()
        .system_tray(system_tray)
        .on_system_tray_event(handle_tray_event)
        .on_window_event(|event| {
            // Hide window instead of closing
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                event.window().hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            let settings = settings.clone();
            let app_handle = app.handle();

            // Start WebSocket server in background
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_server(settings).await {
                    log::error!("WebSocket server error: {}", e);

                    // Notify user of error
                    if let Some(window) = app_handle.get_window("main") {
                        let _ = window.emit("server-error", e.to_string());
                    }
                }
            });

            // Hide the main window by default (runs in tray)
            if let Some(window) = app.get_window("main") {
                let _ = window.hide();
            }

            log::info!("PrintBridge Agent started successfully");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error running PrintBridge");
}

fn create_tray_menu() -> SystemTrayMenu {
    SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("status", "Status: Running").disabled())
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("settings", "Settings"))
        .add_item(CustomMenuItem::new("logs", "View Logs"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("about", "About PrintBridge"))
        .add_item(CustomMenuItem::new("quit", "Quit"))
}

fn handle_tray_event(app: &tauri::AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            // Show/hide main window on left click
            if let Some(window) = app.get_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
        SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
            "settings" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", "settings");
                }
            }
            "logs" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", "logs");
                }
            }
            "about" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("navigate", "about");
                }
            }
            "quit" => {
                log::info!("User requested quit");
                std::process::exit(0);
            }
            _ => {}
        },
        _ => {}
    }
}

async fn start_server(settings: Arc<Settings>) -> anyhow::Result<()> {
    // Create message handler
    let handler = Arc::new(MessageHandler::new(settings.clone())?);

    // Create and start WebSocket server
    let server = WebSocketServer::new(settings, handler);
    server.start().await?;

    Ok(())
}
