#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod balance;
mod cleanup;
mod commands;
mod models;
mod privilege;
mod store;
mod tray;
mod util;

use std::process::Child;
use std::sync::Mutex;
use tauri::Manager;

// ---- 原生守护全生命周期日志（诊断用） ----
// GUI 启动（双击 / LaunchPad / 登录项）的 App 进程没有 tty，stderr 被丢弃，
// 因此必须把生命周期事件落盘到 ~/Library/Logs/，才能在用户真机上事后分析
// 「守护为何没起来」。Swift 侧见 src-tauri/swift/main.swift 的同名日志。
use std::io::Write;

fn now_ts() -> String {
    if let Ok(out) = std::process::Command::new("/bin/date")
        .arg("+%Y-%m-%dT%H:%M:%S%z")
        .output()
    {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                let t = s.trim().to_string();
                if !t.is_empty() {
                    return t;
                }
            }
        }
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", secs)
}

/// 主程序（Rust）侧日志路径。
fn rust_log_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::Path::new(&home)
        .join("Library")
        .join("Logs")
        .join("switchkit-rust.log")
}

/// 守护（Swift）侧日志路径；守护自身的 stderr 也重定向到这里，便于一处收集。
fn daemon_log_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::Path::new(&home)
        .join("Library")
        .join("Logs")
        .join("switchkit-daemon.log")
}

fn lifecycle_log(msg: &str) {
    let path = rust_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) =
        std::fs::OpenOptions::new().create(true).append(true).open(&path)
    {
        let _ = writeln!(f, "[{}] {}", now_ts(), msg);
    }
}

/// 进程是否存活（kill -0 探测，不发送信号）。
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

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
/// 全程向 switchkit-rust.log 写生命周期日志，便于真机事后定位。
fn spawn_sidecar() {
    use std::net::TcpStream;
    use std::time::Duration;
    lifecycle_log("spawn_sidecar: enter");
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(e) => {
            lifecycle_log(&format!("spawn_sidecar: bad addr: {}", e));
            return;
        }
    };
    // 升级/异常退出残留：清掉占用 9876 的旧守护，确保拉起的是本次构建的健康守护。
    // （此前 693dc59 回退时误删了 dffe193 的这步清理，导致升级后原生守护不亮。）
    kill_port_holder();
    lifecycle_log("spawn_sidecar: kill_port_holder done");
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        lifecycle_log("spawn_sidecar: 9876 already listening, reuse existing daemon");
        return; // 已有实例在跑
    }
    lifecycle_log("spawn_sidecar: 9876 not listening, need to launch");
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
        if !path.exists() {
            lifecycle_log(&format!(
                "spawn_sidecar: candidate MISSING: {}",
                path.display()
            ));
            continue;
        }
        lifecycle_log(&format!(
            "spawn_sidecar: candidate exists: {}",
            path.display()
        ));
        // 关键（Apple Silicon 修复）：先清掉嵌套守护自身的隔离属性再 spawn。
        #[cfg(target_os = "macos")]
        strip_quarantine(&path);
        // 优先直接拉起 bundle 内守护（已清隔离，Apple Silicon 上 AMFI 放行）。
        if try_spawn_daemon(&path) {
            lifecycle_log("spawn_sidecar: daemon up via bundle path, done");
            return;
        }
        lifecycle_log("spawn_sidecar: bundle path spawn FAILED, fallback to Application Support");
        // bundle 内仍拉不起（极端环境），退回到 Application Support 副本。
        #[cfg(target_os = "macos")]
        {
            let dest = prepare_daemon_macos(&path);
            if try_spawn_daemon(&dest) {
                lifecycle_log("spawn_sidecar: daemon up via Application Support fallback, done");
                return;
            }
            lifecycle_log("spawn_sidecar: Application Support fallback also FAILED");
        }
        return;
    }
    lifecycle_log("spawn_sidecar: no candidate path found, daemon NOT launched");
}

/// spawn 守护进程并把它的 stderr 重定向到 daemon 日志文件，
/// 探测 9876 是否在 ~1s 内就绪；就绪返回 true。
/// 守护独立常驻：就绪后 std::mem::forget 脱离句柄，由其自带父进程看门狗回收。
/// 未就绪时回收该子进程并记录诊断信息（是否拉起、是否还活着、其 stderr 已落盘）。
fn try_spawn_daemon(path: &std::path::Path) -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    // 守护 stderr 重定向到 daemon 日志：GUI 下守护 stderr 被丢弃，落盘后我们
    // 才能看到它 bind/权限阶段的报错（如被 AMFI 拦截、端口占用）。
    let mut cmd = std::process::Command::new(path);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(daemon_log_path())
    {
        Ok(f) => {
            cmd.stderr(f);
        }
        Err(_) => {
            cmd.stderr(std::process::Stdio::null());
        }
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            lifecycle_log(&format!(
                "try_spawn_daemon: spawn FAILED for {}: {}",
                path.display(),
                e
            ));
            return false;
        }
    };
    let pid = child.id();
    lifecycle_log(&format!(
        "try_spawn_daemon: spawned pid {} ({})",
        pid,
        path.display()
    ));
    let mut up = false;
    for _ in 0..20 {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if up {
        lifecycle_log(&format!(
            "try_spawn_daemon: pid {} listening on 9876 OK",
            pid
        ));
        std::mem::forget(child); // 脱离句柄，由看门狗回收
        true
    } else {
        let alive = is_process_alive(pid);
        lifecycle_log(&format!(
            "try_spawn_daemon: pid {} NOT up after ~1s; alive={}; 其 stderr 见 daemon 日志",
            pid, alive
        ));
        let _ = child.kill();
        let _ = child.wait();
        false
    }
}

/// macOS 专用：把守护复制到用户可写目录并去掉下载隔离属性，返回实际可执行路径。
/// 为什么需要：CI / 邮件 / Release 下载的包经浏览器/邮件保存后会被系统打
/// `com.apple.quarantine`，而本仓库没有 Developer ID 证书、只能 ad-hoc 自签名，
/// 于是 Gatekeeper/AMFI 在 App 拉起这个 sidecar 时拦截它，端口 9876 起不来、原生守护不亮。
/// 复制到 Application Support 后该副本不带隔离属性，与普通 ad-hoc 二进制无异。
///
/// 注意：早期实现放 /tmp，但 macOS 26+ 对脱离 app bundle 的 /tmp 副本反而更严、易被
/// AMFI 拦；Application Support 在用户可写域、非下载隔离树，放行更稳。
/// `std::fs::copy` 在 macOS 上【会】把源文件的 xattr（含 quarantine）一并拷到目标，
/// 所以 copy 之后必须显式去隔离；GUI 启动（双击/LaunchPad/登录项）时进程 PATH 仅含
/// /usr/bin:/bin，裸 `xattr` 不一定可用，故一律用绝对路径 /usr/bin/xattr。
#[cfg(target_os = "macos")]
fn prepare_daemon_macos(src: &std::path::Path) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let dir = std::path::Path::new(&home)
        .join("Library")
        .join("Application Support")
        .join("com.switchkit.app");
    let dest = dir.join("switchkit-daemon");
    std::fs::create_dir_all(&dir).ok();
    // 复制失败（极端情况）则退回原路径，并在原路径上尽量去隔离。
    match std::fs::copy(src, &dest) {
        Ok(_) => {}
        Err(_) => {
            strip_quarantine(src);
            return src.to_path_buf();
        }
    }
    // 兜底：源二进制若仍带隔离也一并清掉（defense-in-depth）
    strip_quarantine(src);
    strip_quarantine(&dest);
    // 确保副本可执行（fs::copy 在某些情形下目标权限可能不含 +x）
    let _ = std::fs::set_permissions(&dest, std::os::unix::fs::PermissionsExt::from_mode(0o755));
    dest
}

/// 清掉下载隔离属性（best-effort，属性不存在时忽略报错）。
/// 必须用绝对路径 /usr/bin/xattr：GUI 启动的 App 进程 PATH 仅 /usr/bin:/bin，
/// 裸 `xattr` 在某些情况下解析不到；同文件里 kill_port_holder 已对 lsof 用过同样的绝对路径写法。
#[cfg(target_os = "macos")]
fn strip_quarantine(path: &std::path::Path) {
    for attr in [
        "com.apple.quarantine",
        "com.apple.metadata:com_apple_quarantine",
    ] {
        let out = std::process::Command::new("/usr/bin/xattr")
            .args(["-d", attr, &path.to_string_lossy()])
            .output();
        match out {
            Ok(o) => lifecycle_log(&format!(
                "strip_quarantine: xattr -d {} on {} -> exited={} ({})",
                attr,
                path.display(),
                o.status,
                if o.status.success() {
                    "已移除隔离属性"
                } else {
                    "属性本就不存在(正常)"
                }
            )),
            Err(e) => lifecycle_log(&format!(
                "strip_quarantine: xattr FAILED for {}: {}",
                path.display(),
                e
            )),
        }
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

            // 配置规范化迁移：把旧字段名（deepseek_keys）归一为 provider_keys，
            // 并剔除已删除功能遗留的未知字段（menubar_* 等）。修复「升级后余额
            // apikey 消失」——此前 schema 漂移会让整文件反序列化失败、清空所有设置。
            store::normalize_settings(app.handle());

            // 包被替换/移动后修正开机自启指向（旧路径会让 launchd 拉起旧副本）
            store::repair_launch_agent();

            // 拉起守护进程（独立常驻，不持有句柄）
            spawn_sidecar();

            // 自动清理定时器：后台线程，启动 ~60s 后首检，之后每小时检查一次。
            // 仅清理 safe 级项；周期与开关来自持久化设置。
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    crate::cleanup::run_auto_clean_if_due(&handle);
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                });
            }

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
            commands::scan_cleanup,
            commands::clean_selected,
            commands::bundle_usage,
            commands::large_files,
            commands::trash_path,
            commands::list_trash_items,
            commands::restore_trash_item,
            commands::empty_switchkit_trash,
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
