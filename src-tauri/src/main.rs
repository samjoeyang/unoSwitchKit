#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod balance;
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
    /// 由本进程拉起的 Swift 守护进程句柄。必须持有：退出时主动终止，
    /// 否则它是 AppKit 常驻进程，会一直占用 .app 包内的可执行文件。
    pub sidecar: Mutex<Option<Child>>,
}

/// 拉起 Swift 守护进程（蓝牙承载），已在运行则跳过。
/// 返回**本进程新建**的子进程句柄；复用已有实例时返回 None。
fn spawn_sidecar() -> Option<Child> {
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return None,
    };
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        return None; // 已有实例在跑（归属其它进程，由它自己回收）
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
            // 传入父 PID：守护进程据此自我看门，App 异常退出时不会变成孤儿
            if let Ok(child) = std::process::Command::new(&path)
                .arg("--parent-pid")
                .arg(std::process::id().to_string())
                .spawn()
            {
                return Some(child);
            }
            break;
        }
    }
    None
}

fn main() {
    let mut builder = tauri::Builder::default()
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

            // 包被替换/移动后修正开机自启指向（旧路径会让 launchd 拉起旧副本）
            store::repair_launch_agent();

            // 先注册状态再拉守护进程：句柄要存进 AppState 以便退出时回收
            let sidecar = spawn_sidecar();
            app.manage(AppState {
                caffeinate: Mutex::new(None),
                screen_cleaned: Mutex::new(false),
                sidecar: Mutex::new(sidecar),
            });
            Ok(())
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
            commands::get_balances,
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
            commands::get_update_config,
        ]);

    // 在线更新：插件始终注册；更新服务在 tauri.conf.json `plugins.updater`
    // 中配置（当前 endpoints 为空 → 暂无线上地址，检查更新返回「已是最新」）。
    // 后续接入服务器时把 endpoints 填上 manifest URL 即可，无需改 Rust 代码。
    builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running SwitchKit");

    // 退出兜底：无论从面板按钮、系统注销还是异常清理路径退出，都先回收守护进程，
    // 避免残留进程占用 .app 内二进制文件（覆盖安装会被系统拒绝）。
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            commands::shutdown_sidecar(handle);
        }
    });
}
