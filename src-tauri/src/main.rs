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
}

/// 清掉占用 9876 端口的进程（best-effort）。用于两处：
/// ① 启动时发现端口被旧版本残留守护占用——旧版无看门狗、且曾有过
///    `bluetooth:status` 挂起 bug（见 06faba6），必须换成本次构建的健康守护，
///    否则状态探测会超时、面板显示「原生守护未运行」；
/// ② 退出兜底（见 commands::shutdown_sidecar）。
pub fn kill_port_holder() {
    // 必须用绝对路径：GUI 启动（双击 / LaunchPad / 登录项）时进程 PATH 仅含
    // /usr/bin:/bin，裸 `lsof` 会 command not found，导致清不掉端口占用。
    let _ = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "/usr/sbin/lsof -nP -tiTCP:9876 -sTCP:LISTEN 2>/dev/null | while read -r p; do kill -9 \"$p\" 2>/dev/null; done",
        ])
        .output();
}

/// 拉起 Swift 守护进程（蓝牙承载），独立于 App 生命周期常驻。
/// 端口已被占用（旧实例在跑）时直接跳过——守护自身带父进程看门狗，
/// 父进程退出即自我回收，无需本进程持有其句柄。
fn spawn_sidecar() {
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return,
    };
    // 升级/异常退出残留：清掉占用 9876 的旧守护，确保拉起的是本次构建的健康守护。
    // 否则会复用残留的旧守护，状态探测超时、面板显示「原生守护未运行」。
    // （此前 693dc59 回退时误删了 dffe193 的这步清理，导致升级后原生守护不亮。）
    kill_port_holder();
    // 清掉后重新探测：若仍有实例在跑（kill 失败等极端情况），复用现有守护。
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
            // 下载的包会被系统打 com.apple.quarantine，ad-hoc 自签名（本仓库无
            // Developer ID 证书）的 sidecar 因此被 Gatekeeper 拦截、9876 起不来、
            // 面板「原生守护」始终不亮。复制到可写目录并去掉隔离属性后再拉起，
            // 是「无证书也能让下载包装亮」的可行解法（无需用户手动 xattr）。
            #[cfg(target_os = "macos")]
            let run_path = prepare_daemon_macos(&path);
            #[cfg(not(target_os = "macos"))]
            let run_path = path.clone();

            // 守护进程独立于 App 生命周期：不持有句柄，避免 Rust 的 Child
            // drop 在运行期释放时 SIGKILL 掉守护；其自带父进程看门狗负责回收。
            if let Ok(child) = std::process::Command::new(&run_path).spawn() {
                std::mem::forget(child);
            }
            break;
        }
    }
}

/// macOS 专用：把守护复制到可写临时目录并去掉下载隔离属性，返回实际可执行路径。
/// 为什么需要：CI / 邮件 / Release 下载的包经浏览器/邮件保存后会被系统打
/// `com.apple.quarantine`，而本仓库没有 Developer ID 证书、只能 ad-hoc 自签名，
/// 于是 Gatekeeper 在 App 拉起这个 sidecar 时拦截它，端口 9876 起不来、原生守护不亮。
/// 复制到 /tmp 后该副本不带隔离属性（Rust 的 std::fs::copy 不携带 xattr，再显式
/// 清一次兜底），与普通 ad-hoc 二进制无异，Gatekeeper 不再拦截。
#[cfg(target_os = "macos")]
fn prepare_daemon_macos(src: &std::path::Path) -> std::path::PathBuf {
    let dest = std::env::temp_dir().join("switchkit-daemon");
    // 复制失败（极端情况）则退回原路径，并在原路径上尽量去隔离。
    if std::fs::copy(src, &dest).is_err() {
        strip_quarantine(src);
        return src.to_path_buf();
    }
    strip_quarantine(&dest);
    dest
}

/// 清掉下载隔离属性（best-effort，属性不存在时忽略报错）。
#[cfg(target_os = "macos")]
fn strip_quarantine(path: &std::path::Path) {
    for attr in [
        "com.apple.quarantine",
        "com.apple.metadata:com_apple_quarantine",
    ] {
        let _ = std::process::Command::new("xattr")
            .args(["-d", attr, &path.to_string_lossy()])
            .output();
    }
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

            // 拉起守护进程（独立常驻，不持有句柄）
            spawn_sidecar();
            app.manage(AppState {
                caffeinate: Mutex::new(None),
                screen_cleaned: Mutex::new(false),
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
