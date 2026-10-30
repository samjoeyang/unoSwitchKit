#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cache_whitelist;
mod commands;
mod models;
mod privilege;
mod store;
mod tray;
mod util;

use std::process::Child;
use std::sync::Mutex;
use tauri::Manager;

/// 全局可变状态：prevent-sleep 的 caffeinate 子进程句柄 + 屏幕清洁状态。
pub struct AppState {
    pub caffeinate: Mutex<Option<Child>>,
    /// 屏幕清洁是否处于「已收起全部前台窗口」状态（本应用维护）。
    pub screen_cleaned: Mutex<bool>,
}

/// 拉起 Swift 守护进程（菜单栏图标管理 / 蓝牙承载），已在运行则跳过。
fn spawn_sidecar() {
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return,
    };
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        return; // 已有实例在跑
    }
    // 产物路径：发布态在 exe 旁；开发态在 src-tauri/binaries/<名>-<triple>
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("switchkit-daemon"));
        }
    }
    let triple = format!("{}-apple-darwin", std::env::consts::ARCH);
    candidates.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("switchkit-daemon-{}", triple)),
    );
    for path in candidates {
        if path.exists() {
            if let Ok(child) = std::process::Command::new(&path).spawn() {
                // 守护进程独立于 App 生命周期，刻意不持有句柄
                std::mem::forget(child);
            }
            break;
        }
    }
}

fn main() {
    tauri::Builder::default()
        // 单实例：launchd 拉起或手动双开时，新进程立即退出并唤起已有面板，
        // 避免「开机自启后菜单栏出现两个图标」。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            // accessory 模式：无 Dock 图标、无 App Switcher 条目
            let _ = app
                .handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);

            // 无边框、置顶、透明面板窗口，默认隐藏，点击托盘时弹出
            let window = tauri::WebviewWindowBuilder::new(
                app.handle(),
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("SwitchKit")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            // 固定弹出面板尺寸；内容超出时由前端容器内部滚动
            .inner_size(320.0, 540.0)
            .build()?;

            // 失焦自动收起面板（点击外部 / 切换到其它 App）
            let win = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = win.hide();
                }
            });

            tray::create_tray(app)?;
            spawn_sidecar();
            Ok(())
        })
        .manage(AppState {
            caffeinate: Mutex::new(None),
            screen_cleaned: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_toggle_states,
            commands::toggle_dark_mode,
            commands::toggle_hide_desktop,
            commands::toggle_prevent_sleep,
            commands::empty_trash,
            commands::toggle_hide_dock,
            commands::toggle_show_hidden_files,
            commands::toggle_screen_clean,
            commands::toggle_stage_manager,
            commands::lock_screen,
            commands::clear_clipboard,
            commands::start_screensaver,
            commands::close_all_apps,
            commands::release_memory,
            commands::flush_dns,
            commands::preview_cache_cleanup,
            commands::clear_cache,
            commands::get_memory_stats,
            commands::get_settings,
            commands::save_settings,
            commands::set_launch_at_login,
            commands::is_launch_at_login,
            commands::sidecar_status,
            commands::bt_devices,
            commands::bt_connect,
            commands::bt_disconnect,
            commands::quit_app,
            commands::menubar_state,
            commands::menubar_set_visible,
            commands::menubar_set_managed,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SwitchKit");
}
