use std::{
    env,
    sync::atomic::{AtomicBool, Ordering},
};

use tauri::{
    AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::{app::AppContext, error::AppError};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_OPEN_ID: &str = "open";
const TRAY_QUIT_ID: &str = "quit";
const AUTOSTART_ARG: &str = "--autostart";
const DEFAULT_WINDOW_WIDTH: f64 = 1000.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 650.0;
const MIN_WINDOW_WIDTH: f64 = 900.0;
const MIN_WINDOW_HEIGHT: f64 = 600.0;
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn run(context: AppContext) -> Result<(), AppError> {
    let launched_via_autostart = env::args().any(|arg| arg == AUTOSTART_ARG);
    let mut builder = tauri::Builder::default()
        .manage(context)
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
        ]);

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = show_main_window(app);
        }));
    }

    let app = builder
        .setup(move |app| {
            let handle = app.handle();
            create_tray(&handle)?;
            if launched_via_autostart {
                destroy_main_window(&handle)?;
            } else {
                show_main_window(&handle)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|error| AppError::DesktopShell(error.to_string()))?;

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, .. } = event
            && !EXIT_REQUESTED.load(Ordering::Relaxed)
        {
            api.prevent_exit();
            let _ = destroy_main_window(app);
        }
    });

    Ok(())
}

fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(MAIN_WINDOW_LABEL).is_some() {
        return Ok(());
    }

    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::default())
        .title("AirSlate 控制台")
        .inner_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
        .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        .resizable(true)
        .center()
        .build()?;

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
                let _ = show_main_window(app);
            }
            TRAY_QUIT_ID => {
                EXIT_REQUESTED.store(true, Ordering::Relaxed);
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
                let _ = show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window
    } else {
        create_main_window(app)?;
        app.get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| tauri::Error::AssetNotFound(MAIN_WINDOW_LABEL.to_string()))?
    };

    window.restore_state(window_state_flags())?;
    window.show()?;
    window.unminimize()?;
    window.set_focus()?;
    Ok(())
}

fn window_state_flags() -> StateFlags {
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED
}

fn destroy_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy()?;
    }
    Ok(())
}
