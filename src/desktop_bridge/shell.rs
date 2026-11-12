use std::{env, sync::mpsc, thread};

use tauri::{
    AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::{
    app::{self, lifecycle::SESSION_STATUS_CHANGED_EVENT},
    error::AppError,
    usb_accessory::USB_STATUS_CHANGED_EVENT,
};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_OPEN_ID: &str = "open";
const TRAY_QUIT_ID: &str = "quit";
const AUTOSTART_ARG: &str = "--autostart";

struct WindowRequests(mpsc::Sender<()>);

pub fn run() -> Result<(), AppError> {
    let launched_via_autostart = is_autostart(env::args());
    let (window_sender, window_receiver) = mpsc::channel();
    // Acquire the instance lock before initializing input, USB, or network services.
    // Queue activation requests even while the primary instance is still in setup.
    let builder = tauri::Builder::default()
        .manage(WindowRequests(window_sender))
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !is_autostart(args.into_iter()) {
                request_main_window(app);
            }
        }));
    let app = builder
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .skip_initial_state(MAIN_WINDOW_LABEL)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            crate::desktop_bridge::commands::get_app_bootstrap,
            crate::desktop_bridge::commands::open_external,
            crate::desktop_bridge::commands::set_selected_monitor,
            crate::desktop_bridge::commands::set_pressure_curve,
            crate::desktop_bridge::commands::set_launch_at_startup,
            crate::desktop_bridge::commands::set_show_launch_at_startup_on_main_page,
            crate::desktop_bridge::commands::set_latest_contact_move_only,
            crate::desktop_bridge::commands::set_latest_contact_move_tolerance_ms,
            crate::desktop_bridge::commands::set_hover_move_policy,
            crate::desktop_bridge::commands::set_preempt_previous_stroke,
            crate::desktop_bridge::commands::set_precise_anchor_correction_enabled,
            crate::desktop_bridge::commands::set_usb_interface,
            crate::desktop_bridge::commands::set_wired_connection_enabled,
            crate::desktop_bridge::commands::disconnect_active_session,
            crate::desktop_bridge::commands::retry_usb_connection,
            crate::desktop_bridge::commands::scan_usb_devices,
            crate::desktop_bridge::commands::get_lan_ipv4_values,
            crate::desktop_bridge::commands::select_shortcut_preset,
            crate::desktop_bridge::commands::create_shortcut_preset,
            crate::desktop_bridge::commands::rename_shortcut_preset,
            crate::desktop_bridge::commands::delete_shortcut_preset,
            crate::desktop_bridge::commands::reset_shortcut_preset,
            crate::desktop_bridge::commands::set_binding_keys,
            crate::desktop_bridge::commands::set_binding_special_action,
            crate::desktop_bridge::commands::set_radial_outer_slot,
            crate::desktop_bridge::commands::set_radial_inner_bindings,
            crate::desktop_bridge::commands::set_radial_inner_enabled
        ])
        .setup(move |app| {
            let context = app::initialize()?;
            let session_status_events = context.session_lifecycle.status_bus().subscribe()?;
            let usb_status_events = context.usb_status_bus.subscribe();
            app::start_services(&context)?;
            app.manage(context);
            let handle = app.handle();
            let event_handle = handle.clone();
            thread::spawn(move || {
                while let Ok(event) = session_status_events.recv() {
                    if let Err(error) = event_handle.emit(SESSION_STATUS_CHANGED_EVENT, event) {
                        tracing::warn!(error = %error, "session status event delivery stopped");
                        break;
                    }
                }
            });
            let usb_event_handle = handle.clone();
            thread::spawn(move || {
                while let Ok(event) = usb_status_events.recv() {
                    if let Err(error) = usb_event_handle.emit(USB_STATUS_CHANGED_EVENT, event) {
                        tracing::warn!(error = %error, "USB status event delivery stopped");
                        break;
                    }
                }
            });
            create_tray(handle)?;
            tracing::info!(launched_via_autostart, "desktop tray ready");
            start_window_requests(handle, window_receiver)?;
            if !launched_via_autostart {
                request_main_window(handle);
            }
            tracing::info!("desktop shell ready");
            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|error| AppError::DesktopShell(error.to_string()))?;

    app.run(|_app, event| {
        if let RunEvent::ExitRequested { code, api, .. } = event
            && should_keep_running(code)
        {
            api.prevent_exit();
        }
    });

    Ok(())
}

fn is_autostart(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|arg| arg == AUTOSTART_ARG)
}

fn should_keep_running(exit_code: Option<i32>) -> bool {
    exit_code.is_none()
}

fn start_window_requests(app: &AppHandle, receiver: mpsc::Receiver<()>) -> Result<(), AppError> {
    let handle = app.clone();
    // WebView2 creation must run outside synchronous native event handlers.
    // One consumer also serializes simultaneous tray and second-launch requests.
    thread::Builder::new()
        .name("main-window".into())
        .spawn(move || {
            while receiver.recv().is_ok() {
                if let Err(error) = create_main_window(&handle) {
                    tracing::error!(%error, "failed to create the main window");
                    continue;
                }
                // Restore window state on the event loop so its cache lock cannot
                // block a close event while this thread waits for a native window call.
                let event_handle = handle.clone();
                if let Err(error) = handle.run_on_main_thread(move || {
                    if let Err(error) = show_main_window(&event_handle) {
                        tracing::error!(%error, "failed to show the main window");
                    }
                }) {
                    tracing::error!(%error, "failed to dispatch the main window activation");
                }
            }
        })?;
    Ok(())
}

fn request_main_window(app: &AppHandle) {
    if let Err(error) = app.state::<WindowRequests>().0.send(()) {
        tracing::error!(%error, "main window request handler stopped");
    }
}

fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == MAIN_WINDOW_LABEL)
        .ok_or_else(|| tauri::Error::AssetNotFound(MAIN_WINDOW_LABEL.to_string()))?;
    WebviewWindowBuilder::from_config(app, config)?.build()?;

    Ok(())
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id(TRAY_OPEN_ID, "打开界面").build(app)?;
    let quit = MenuItemBuilder::with_id(TRAY_QUIT_ID, "退出程序").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_OPEN_ID => {
                request_main_window(app);
            }
            TRAY_QUIT_ID => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                request_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| tauri::Error::AssetNotFound(MAIN_WINDOW_LABEL.to_string()))?;

    window.restore_state(window_state_flags())?;
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    tracing::info!("main window activated");
    Ok(())
}

fn window_state_flags() -> StateFlags {
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_the_last_window_keeps_the_server_running() {
        assert!(should_keep_running(None));
    }

    #[test]
    fn explicit_exit_and_restart_requests_are_allowed() {
        assert!(!should_keep_running(Some(0)));
        assert!(!should_keep_running(Some(1)));
        assert!(!should_keep_running(Some(tauri::RESTART_EXIT_CODE)));
    }

    #[test]
    fn autostart_does_not_open_the_console() {
        assert!(is_autostart(
            ["airslate_pc_server", AUTOSTART_ARG]
                .map(String::from)
                .into_iter()
        ));
        assert!(!is_autostart(
            ["airslate_pc_server"].map(String::from).into_iter()
        ));
    }
}
