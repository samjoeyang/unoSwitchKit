use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, State};

use crate::util;
use crate::AppState;
use crate::models::*;

// ---------------- 状态读取 ----------------

fn is_dark_mode() -> bool {
    // AppleInterfaceStyle 存在且为 Dark 即深色模式；不存在视为浅色
    match util::run("defaults", &["read", "-g", "AppleInterfaceStyle"]) {
        Ok(out) => out.contains("Dark"),
        Err(_) => false,
    }
}

/// 「自动」外观开关：AppleInterfaceStyleSwitchesAutomatically。
/// 该键与 AppleInterfaceStyle 可以同时存在（本机 12.7.6 实测 auto=1 + Dark），
/// 所以判断外观必须先看它，否则 Auto 模式下会把当前生效外观误报成手动深色。
fn is_auto_appearance() -> bool {
    util::run(
        "defaults",
        &["read", "-g", "AppleInterfaceStyleSwitchesAutomatically"],
    )
    .map(|out| matches!(out.trim(), "1" | "true" | "TRUE"))
    .unwrap_or(false)
}

pub fn appearance_mode() -> AppearanceMode {
    if is_auto_appearance() {
        AppearanceMode::Auto
    } else if is_dark_mode() {
        AppearanceMode::Dark
    } else {
        AppearanceMode::Light
    }
}

/// 写全局偏好后广播主题变更通知，尽力让系统立即重算外观。
///
/// 为什么需要：系统设置面板切换外观时除了写 .GlobalPreferences，还会发
/// AppleInterfaceThemeChangedNotification；只写 defaults 时部分系统版本要等到
/// 注销/重新登录才刷新。这里用 CoreFoundation 的 distributed center 补发同名通知，
/// 失败也无害（偏好已落盘，最迟下次登录或系统日程判断时生效）。
#[cfg(target_os = "macos")]
mod theme_notify {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_void};

    type CFStringRef = *const c_void;
    type CFNotificationCenterRef = *const c_void;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
        fn CFNotificationCenterGetDistributedCenter() -> CFNotificationCenterRef;
        fn CFNotificationCenterPostNotification(
            center: CFNotificationCenterRef,
            name: CFStringRef,
            object: *const c_void,
            user_info: *const c_void,
            deliver_immediately: bool,
        );
    }

    pub fn post_theme_changed() {
        unsafe {
            let Ok(raw) = CString::new("AppleInterfaceThemeChangedNotification") else {
                return;
            };
            let name = CFStringCreateWithCString(
                std::ptr::null(),
                raw.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            );
            if name.is_null() {
                return;
            }
            let center = CFNotificationCenterGetDistributedCenter();
            if !center.is_null() {
                CFNotificationCenterPostNotification(
                    center,
                    name,
                    std::ptr::null(),
                    std::ptr::null(),
                    true,
                );
            }
            CFRelease(name);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod theme_notify {
    pub fn post_theme_changed() {}
}

/// 设置外观三态。浅/深沿用现有 System Events AppleScript（即时生效并广播通知），
/// 自动只写系统偏好键 —— 由 macOS 自己按日出/日落或夜览日程切换。
pub fn apply_appearance_mode(mode: AppearanceMode) -> Result<(), String> {
    match mode {
        AppearanceMode::Auto => {
            util::run(
                "defaults",
                &[
                    "write",
                    "-g",
                    "AppleInterfaceStyleSwitchesAutomatically",
                    "-bool",
                    "true",
                ],
            )?;
        }
        AppearanceMode::Light | AppearanceMode::Dark => {
            // 先退出 Auto，否则系统日程会很快把手动选择覆盖回去。
            util::run(
                "defaults",
                &[
                    "write",
                    "-g",
                    "AppleInterfaceStyleSwitchesAutomatically",
                    "-bool",
                    "false",
                ],
            )?;
            let script = format!(
                "tell application \"System Events\" to tell appearance preferences to set dark mode to {}",
                if mode == AppearanceMode::Dark { "true" } else { "false" }
            );
            util::run_osascript(&script)?;
        }
    }
    theme_notify::post_theme_changed();
    Ok(())
}

fn is_desktop_hidden() -> bool {
    // CreateDesktop = false 表示隐藏桌面图标；不存在视为显示
    match util::run("defaults", &["read", "com.apple.finder", "CreateDesktop"]) {
        Ok(out) => out.trim() == "0",
        Err(_) => false,
    }
}

fn trash_dir() -> std::path::PathBuf {
    util::home_dir().join(".Trash")
}

fn trash_count() -> u32 {
    // 以 Finder 为准：与系统废纸篓显示完全一致，规避直接读 ~/.Trash 的
    // 权限（TCC）/特例导致的漏计。首次调用可能弹一次"控制 Finder"授权。
    if let Ok(out) = util::run_osascript("tell application \"Finder\" to return count of items in trash") {
        if let Ok(n) = out.trim().parse::<u32>() {
            return n;
        }
    }
    // 回退：直接枚举 ~/.Trash
    let trash = trash_dir();
    if !trash.exists() {
        return 0;
    }
    fs::read_dir(&trash)
        .map(|entries| entries.filter_map(|e| e.ok()).count() as u32)
        .unwrap_or(0)
}

fn is_prevent_sleep(state: &AppState) -> bool {
    let mut guard = state.caffeinate.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                false
            } // 已退出
            Ok(None) => true,  // 仍在运行
            Err(_) => {
                *guard = None;
                false
            }
        }
    } else {
        false
    }
}

fn is_hide_dock() -> bool {
    match util::run("defaults", &["read", "com.apple.dock", "autohide"]) {
        Ok(out) => out.trim() == "1",
        Err(_) => false,
    }
}

fn is_show_hidden_files() -> bool {
    match util::run("defaults", &["read", "com.apple.finder", "AppleShowAllFiles"]) {
        Ok(out) => out.trim() == "1",
        Err(_) => false,
    }
}

fn is_stage_manager() -> bool {
    match util::run("defaults", &["read", "com.apple.WindowManager", "GloballyEnabled"]) {
        Ok(out) => out.trim() == "1",
        Err(_) => false,
    }
}

/// 主系统版本（如 12.7.6 → 12）。台前调度仅 13+ 可用。
fn os_major_version() -> u32 {
    util::run("sw_vers", &["-productVersion"])
        .ok()
        .and_then(|s| {
            s.trim()
                .split('.')
                .next()
                .and_then(|m| m.parse::<u32>().ok())
        })
        .unwrap_or(0)
}

/// AppleScript 里跳过自身进程的守卫条件（dev 产物名与发布名都覆盖）。
fn skip_own_app_condition() -> String {
    format!(
        "(unix id of p) is not {}",
        std::process::id()
    )
}

// ---------------- 内存统计 ----------------

fn parse_pages(s: &str) -> u64 {
    s.trim().trim_end_matches('.').trim().parse::<u64>().unwrap_or(0)
}

/// vm_stat 首行形如 `Mach Virtual Memory Statistics: (page size of 16384 bytes)`。
/// Apple Silicon 为 16384，Intel 为 4096；硬编码 4096 会让 aarch64 上的内存
/// 数字只有真实值的 1/4，必须按输出里的实际页大小换算。
fn parse_page_size(vm: &str) -> Option<u64> {
    let (_, rest) = vm.lines().next()?.split_once("page size of")?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u64>().ok().filter(|n| *n > 0)
}

/// vm_stat 首行格式变化时的兜底：直接问内核页大小。
fn system_page_size() -> u64 {
    util::run("sysctl", &["-n", "hw.pagesize"])
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(4096)
}

#[derive(Default)]
struct VmPages {
    free: u64,
    active: u64,
    inactive: u64,
    speculative: u64,
    wired: u64,
    compressed: u64,
}

fn parse_vm_pages(vm: &str) -> VmPages {
    let mut p = VmPages::default();
    for line in vm.lines() {
        let v = line.split_once(':').map(|(_, r)| r).unwrap_or("");
        if line.starts_with("Pages free") {
            p.free = parse_pages(v);
        } else if line.starts_with("Pages active") {
            p.active = parse_pages(v);
        } else if line.starts_with("Pages inactive") {
            p.inactive = parse_pages(v);
        } else if line.starts_with("Pages speculative") {
            p.speculative = parse_pages(v);
        } else if line.starts_with("Pages wired down") {
            p.wired = parse_pages(v);
        } else if line.starts_with("Pages occupied by compressor") {
            p.compressed = parse_pages(v);
        }
    }
    p
}

/// 与资源面板 `sysinfo::System::used_memory()` 保持同一口径：
/// `used = active + wired + compressor + speculative`（活动监视器的「已用」），
/// **不含** inactive 可回收缓存。inactive 单独放进 `cached_mb`，用来衡量
/// `purge` 到底回收了多少缓存。两处口径不一致会让同一时刻出现
/// 「面板 8.9 GB / 日志 12.56 GB」这类相差数 GB 的数字。
fn memory_stat(p: &VmPages, page_size: u64, total_bytes: u64) -> MemoryStat {
    let mb = |pages: u64| pages.saturating_mul(page_size) / 1_048_576;
    MemoryStat {
        // free_count 已包含 speculative，不再重复相加
        free_mb: mb(p.free),
        used_mb: mb(p.active + p.wired + p.compressed + p.speculative),
        total_mb: total_bytes / 1_048_576,
        compressed_mb: mb(p.compressed),
        cached_mb: mb(p.inactive),
    }
}

fn parse_memory() -> Result<MemoryStat, String> {
    let vm = util::run("vm_stat", &[])?;
    let page_size = parse_page_size(&vm).unwrap_or_else(system_page_size);
    let total_out = util::run("sysctl", &["-n", "hw.memsize"])?;
    let total_bytes: u64 = total_out
        .trim()
        .parse()
        .map_err(|_| "解析总内存失败".to_string())?;
    Ok(memory_stat(&parse_vm_pages(&vm), page_size, total_bytes))
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    const INTEL: &str = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
Pages free: 4993.\n\
Pages active: 1602108.\n\
Pages inactive: 1393753.\n\
Pages speculative: 208894.\n\
Pages wired down: 765716.\n\
Pages occupied by compressor: 218347.\n";

    const ARM: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
Pages free: 1000.\n\
Pages active: 100000.\n\
Pages inactive: 200000.\n\
Pages speculative: 10000.\n\
Pages wired down: 50000.\n\
Pages occupied by compressor: 20000.\n";

    #[test]
    fn page_size_is_read_from_vm_stat() {
        assert_eq!(parse_page_size(INTEL), Some(4096));
        assert_eq!(parse_page_size(ARM), Some(16384));
        assert_eq!(parse_page_size("no header here"), None);
    }

    #[test]
    fn arm_page_size_is_not_undercounted() {
        let p = parse_vm_pages(ARM);
        let s = memory_stat(&p, 16384, 16 * 1_073_741_824);
        // 旧实现按 4096 换算只有这里的 1/4
        assert_eq!(s.cached_mb, 200000 * 16384 / 1_048_576);
        assert_eq!(s.total_mb, 16384);
    }

    /// 回归护栏：本机实测两者只差十余 MB。若页大小再次被硬编码成 4096
    /// （Apple Silicon 上是 16384），或 `used_mb` 又把 inactive 算进去，
    /// 这个差值会立刻放大到数 GB。
    #[test]
    fn live_used_matches_sysinfo() {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();
        let s = parse_memory().unwrap();
        let sysinfo_mb = sys.used_memory() / 1_048_576;
        let diff = sysinfo_mb.abs_diff(s.used_mb);
        assert!(
            diff < 1024,
            "两套统计口径应一致，实际相差 {} MB（sysinfo {} MB / vm_stat {} MB）",
            diff,
            sysinfo_mb,
            s.used_mb
        );
    }

    #[test]
    fn used_excludes_inactive_but_keeps_speculative() {
        let p = parse_vm_pages(INTEL);
        assert_eq!(p.inactive, 1_393_753);
        assert_eq!(p.speculative, 208_894);
        let s = memory_stat(&p, 4096, 16 * 1_073_741_824);
        assert_eq!(s.used_mb, (1_602_108 + 765_716 + 218_347 + 208_894) * 4096 / 1_048_576);
        assert_eq!(s.cached_mb, 1_393_753 * 4096 / 1_048_576);
        assert_eq!(s.free_mb, 4993 * 4096 / 1_048_576);
    }
}

// ---------------- 命令 ----------------

/// 注：本命令涉及十余次外部进程调用（defaults/osascript/sw_vers），必须 async
/// 在线程池执行——同步命令跑在主线程上，会令面板出现彩虹圈。
#[tauri::command]
pub async fn get_toggle_states(state: State<'_, AppState>) -> Result<ToggleStates, String> {
    let major = os_major_version();
    let s = state.inner();
    let screen_clean = s.screen_cleaned.lock().map(|g| *g).unwrap_or(false);
    Ok(ToggleStates {
        dark_mode: is_dark_mode(),
        appearance_mode: appearance_mode().as_str().to_string(),
        desktop_hidden: is_desktop_hidden(),
        prevent_sleep: is_prevent_sleep(s),
        trash_count: trash_count(),
        hide_dock: is_hide_dock(),
        show_hidden_files: is_show_hidden_files(),
        screen_clean,
        stage_manager: if major >= 13 { is_stage_manager() } else { false },
        stage_manager_available: major >= 13,
    })
}

/// 外观三态：读取（轻量，前端轮询用，不跑全套 get_toggle_states）。
#[tauri::command]
pub async fn get_appearance_mode() -> String {
    appearance_mode().as_str().to_string()
}

#[tauri::command]
pub async fn set_appearance_mode(mode: String) -> Result<String, String> {
    let target = AppearanceMode::from_str_opt(&mode)
        .ok_or_else(|| format!("未知外观模式：{}", mode))?;
    apply_appearance_mode(target)?;
    Ok(appearance_mode().as_str().to_string())
}

/// 面板「外观」按钮与全局快捷键共用：浅 → 深 → 自动 → 浅。
#[tauri::command]
pub async fn cycle_appearance_mode() -> Result<String, String> {
    apply_appearance_mode(appearance_mode().cycle())?;
    Ok(appearance_mode().as_str().to_string())
}

/// 兼容旧入口：在浅/深之间切换并退出自动模式。
#[tauri::command]
pub async fn toggle_dark_mode() -> Result<bool, String> {
    let next = if appearance_mode() == AppearanceMode::Dark {
        AppearanceMode::Light
    } else {
        AppearanceMode::Dark
    };
    apply_appearance_mode(next)?;
    Ok(next == AppearanceMode::Dark)
}

/// 拉起截屏录屏工具栏（⌘⇧5 的同一个 App）。
///
/// 该工具栏同时提供静态截图与屏幕录制两组按钮，一个入口覆盖两项功能。
/// 系统 App 自带 com.apple.private.screencapture.allow 与
/// com.apple.private.tcc.allow = [kTCCServiceScreenCapture]，采集/编码/保存全由它负责。
///
/// 关键：必须用 LaunchServices 启动系统 App，不能 spawn /usr/sbin/screencapture。
/// 本机实测该 CLI 带 com.apple.private.tcc.check-allow-on-responsible-process，
/// TCC 检查的是调用者；从本 App spawn 会被要求屏幕录制权限，且未授权时静默产出黑图。
/// 走 `open -b` 时采集由系统 App 负责，本应用零新增权限、不保存任何截图或录屏文件。
#[tauri::command]
pub async fn open_system_screenshot() -> Result<(), String> {
    if util::run("/usr/bin/open", &["-b", "com.apple.screenshot.launcher"]).is_ok() {
        return Ok(());
    }
    // 极老系统 / LaunchServices 未登记时的兜底：按名称与路径打开。
    if util::run("/usr/bin/open", &["-a", "Screenshot"]).is_ok() {
        return Ok(());
    }
    util::run(
        "/usr/bin/open",
        &["/System/Applications/Utilities/Screenshot.app"],
    )
    .map(|_| ())
    .map_err(|e| format!("无法打开截屏录屏工具栏：{}", e))
}

#[tauri::command]
pub async fn toggle_hide_desktop() -> Result<bool, String> {
    let next_hidden = !is_desktop_hidden();
    let val = if next_hidden { "false" } else { "true" };
    util::run(
        "defaults",
        &["write", "com.apple.finder", "CreateDesktop", "-bool", val],
    )?;
    util::run("killall", &["Finder"])?;
    Ok(next_hidden)
}

#[tauri::command]
pub async fn toggle_prevent_sleep(state: State<'_, AppState>) -> Result<bool, String> {
    let mut guard = state.caffeinate.lock().unwrap();
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        return Ok(false);
    }
    let child = std::process::Command::new("caffeinate")
        .args(["-dimsu"])
        .spawn()
        .map_err(|e| format!("启动 caffeinate 失败: {}", e))?;
    *guard = Some(child);
    Ok(true)
}

#[tauri::command]
pub async fn empty_trash() -> Result<u32, String> {
    let before = trash_count();
    // 优先走 Finder 清空：系统级行为，能处理锁定/权限特殊的文件
    if util::run_osascript("tell application \"Finder\" to empty trash").is_ok() {
        return Ok(before);
    }
    // 回退：直接删除 ~/.Trash 内条目
    let trash = trash_dir();
    if trash.exists() {
        for entry in fs::read_dir(&trash).map_err(|e| e.to_string())? {
            if let Ok(entry) = entry {
                let p = entry.path();
                let _ = fs::remove_dir_all(&p).or_else(|_| fs::remove_file(&p));
            }
        }
    }
    Ok(before)
}

#[tauri::command]
pub async fn toggle_hide_dock() -> Result<bool, String> {
    let next = !is_hide_dock();
    let val = if next { "true" } else { "false" };
    util::run("defaults", &["write", "com.apple.dock", "autohide", "-bool", val])?;
    util::run("killall", &["Dock"])?;
    Ok(next)
}

#[tauri::command]
pub async fn toggle_show_hidden_files() -> Result<bool, String> {
    let next = !is_show_hidden_files();
    let val = if next { "true" } else { "false" };
    util::run(
        "defaults",
        &["write", "com.apple.finder", "AppleShowAllFiles", "-bool", val],
    )?;
    util::run("killall", &["Finder"])?;
    Ok(next)
}

#[tauri::command]
pub async fn toggle_screen_clean(state: State<'_, AppState>) -> Result<bool, String> {
    let mut cleaned = state.screen_cleaned.lock().map_err(|_| "状态锁中毒")?;
    if *cleaned {
        // 恢复：把所有「无 UI 豁免、当前不可见」的常规 App 重新显示
        let script = r#"tell application "System Events"
  repeat with p in (every application process whose background only is false and visible is false)
    try
      set visible of p to true
    end try
  end repeat
end tell"#;
        util::run_osascript(script)?;
        *cleaned = false;
        return Ok(false);
    }
    // 清洁：收起所有前台可见窗口（跳过自身）
    let script = format!(
        r#"tell application "System Events"
  repeat with p in (every application process whose background only is false and visible is true)
    try
      if {} then set visible of p to false
    end try
  end repeat
end tell"#,
        skip_own_app_condition()
    );
    util::run_osascript(&script)?;
    *cleaned = true;
    Ok(true)
}

#[tauri::command]
pub async fn toggle_stage_manager() -> Result<bool, String> {
    if os_major_version() < 13 {
        return Err("台前调度需要 macOS 13 (Ventura) 及以上".to_string());
    }
    let next = !is_stage_manager();
    let val = if next { "true" } else { "false" };
    // Ventura 上经 System Settings 写入的同一偏好键；改后重启 Dock 生效
    util::run(
        "defaults",
        &["write", "com.apple.WindowManager", "GloballyEnabled", "-bool", val],
    )?;
    util::run("killall", &["Dock"])?;
    Ok(next)
}

// ---------------- 蓝牙设备（IOBluetooth，经 Swift 守护） ----------------

/// 枚举已配对蓝牙设备。
#[tauri::command]
pub async fn bt_devices() -> Result<Vec<BtDevice>, String> {
    let v = sidecar_call(json!({ "cmd": "bt:devices" }), 500, 3000)?;
    let devices = v.get("devices").cloned().unwrap_or_else(|| json!([]));
    serde_json::from_value(devices).map_err(|e| e.to_string())
}

/// 一键连接（守护侧轮询等待，最长 10s）。
#[tauri::command]
pub async fn bt_connect(address: String) -> Result<bool, String> {
    let v = sidecar_call(
        json!({ "cmd": "bt:connect", "address": address }),
        500,
        15000,
    )?;
    Ok(v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false))
}

/// 断开连接。
#[tauri::command]
pub async fn bt_disconnect(address: String) -> Result<bool, String> {
    let v = sidecar_call(
        json!({ "cmd": "bt:disconnect", "address": address }),
        500,
        5000,
    )?;
    Ok(v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false))
}

// ---------------- 应用退出 ----------------

/// 终止本进程拉起的 Swift 守护进程（幂等；复用外部实例时无句柄，不做处理）。
/// 守护进程是 AppKit 常驻进程，不主动回收会占住 .app 包内二进制文件，
/// 表现为「点了退出，但覆盖安装新版本时提示正在运行」。
pub fn shutdown_sidecar(_app: &AppHandle) {
    // 先停看门狗：否则清掉端口后它会把守护重新拉起，导致退出后进程残留。
    crate::stop_sidecar_watchdog();
    // 守护进程由自身父进程看门狗在 App 退出时回收，本进程不再持有其句柄；
    // 端口兜底：清掉旧版本遗留的守护进程，避免升级后仍被 9876 占用。
    crate::kill_port_holder();
}

/// 退出 SwitchKit：先回收守护进程，再退出主进程。
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    shutdown_sidecar(&app);
    // 硬退兜底：Tauri 的退出清理在 transparent + always_on_top 窗口下偶发挂住，
    // 3 秒后进程仍在则强退，保证「点退出」一定有结果。
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(3));
        std::process::exit(0);
    });
    app.exit(0);
}

// ---------------- 即时动作 ----------------

/// 锁定屏幕：CGSession -suspend（Monterey 可用），失败回退熄屏。
#[tauri::command]
pub async fn lock_screen() -> Result<(), String> {
    util::run(
        "/System/Library/CoreServices/Menu Extras/User.menu/Contents/Resources/CGSession",
        &["-suspend"],
    )
    .or_else(|_| util::run("pmset", &["displaysleepnow"]))
    .map(|_| ())
}

/// 清空剪贴板。
#[tauri::command]
pub async fn clear_clipboard() -> Result<(), String> {
    util::run("sh", &["-c", ": | pbcopy"]).map(|_| ())
}

/// 启动屏幕保护。
#[tauri::command]
pub async fn start_screensaver() -> Result<(), String> {
    util::run("open", &["-a", "ScreenSaverEngine"]).map(|_| ())
}

/// 优雅退出所有前台 App（跳过自身与 Finder），返回已请求退出的数量。
#[tauri::command]
pub async fn close_all_apps() -> Result<u32, String> {
    let script = format!(
        r#"tell application "System Events"
  set quitCount to 0
  repeat with p in (every application process whose background only is false and visible is true)
    try
      if {} and (name of p) is not "Finder" then
        tell p to quit
        set quitCount to quitCount + 1
      end if
    end try
  end repeat
  return quitCount
end tell"#,
        skip_own_app_condition()
    );
    // -t 15：未保存文档的 App 会弹存盘框，限制每个 AppleEvent 等待 15s，避免面板卡死
    let out = util::run("osascript", &["-e", &script, "-t", "15"])?;
    let n = out.trim().parse::<u32>().unwrap_or(0);
    Ok(n)
}

#[tauri::command]
pub async fn release_memory() -> Result<MemoryComparison, String> {
    let before = parse_memory()?;
    crate::privilege::run_as_root("purge")?;
    let after = parse_memory()?;
    Ok(MemoryComparison { before, after })
}

#[tauri::command]
pub async fn get_system_stats(state: State<'_, AppState>) -> Result<SystemStats, String> {
    let mut mon = state
        .sysmon
        .lock()
        .map_err(|_| "锁系统监控状态失败".to_string())?;
    Ok(mon.sample())
}

#[tauri::command]
pub async fn flush_dns() -> Result<(), String> {
    crate::privilege::run_as_root("dscacheutil -flushcache; killall -HUP mDNSResponder")?;
    Ok(())
}

#[tauri::command]
pub async fn scan_cleanup(app: AppHandle) -> CleanPreview {
    let s = crate::store::load_settings(&app);
    crate::cleanup::scan(&s.cache_whitelist_extra, &s.exclude_paths, s.downloads_expire_days)
}

#[tauri::command]
pub async fn clean_selected(app: AppHandle, ids: Vec<String>) -> CleanResult {
    let s = crate::store::load_settings(&app);
    crate::cleanup::clean(
        &ids,
        &s.cache_whitelist_extra,
        &s.exclude_paths,
        s.downloads_expire_days,
    )
}

#[tauri::command]
pub async fn bundle_usage() -> Vec<BundleUsage> {
    crate::cleanup::bundle_usage()
}

#[tauri::command]
pub async fn large_files(threshold_mb: u64) -> Vec<LargeFile> {
    let threshold = if threshold_mb == 0 {
        500
    } else {
        threshold_mb
    } * 1024
        * 1024;
    crate::cleanup::large_files(threshold)
}

/// 将任意位于主目录内的路径移入废纸篓（用于大文件等定向清理）。
#[tauri::command]
pub async fn trash_path(path: String) -> Result<String, String> {
    crate::cleanup::trash_path(&path)
}

/// 列出本应用移入废纸篓的项（回收站面板）。
#[tauri::command]
pub async fn list_trash_items() -> Vec<TrashItem> {
    crate::cleanup::list_trash_items()
}

/// 从废纸篓恢复某项回原路径（覆盖前校验原路径不存在）。
#[tauri::command]
pub async fn restore_trash_item(trashed_path: String) -> Result<(), String> {
    crate::cleanup::restore_trash_item(&trashed_path)
}

/// 清空本应用移入废纸篓的所有项（仅删 `switchkit-` 前缀，不动系统其它废纸篓文件）。
#[tauri::command]
pub async fn empty_switchkit_trash() -> u32 {
    crate::cleanup::empty_switchkit_trash()
}

/// 查询所有已配置 Key 的余额（无 Key 时返回空列表）。
/// curl 为阻塞调用，放到 blocking 线程池，避免占用 async runtime 工作线程。
#[tauri::command]
pub async fn get_balances(app: AppHandle) -> Vec<AccountBalance> {
    let s = crate::store::load_settings(&app);
    let keys = s.provider_keys;
    let proxy = s.http_proxy;
    tauri::async_runtime::spawn_blocking(move || crate::balance::fetch_all(&keys, &proxy))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn get_memory_stats() -> Result<MemoryStat, String> {
    parse_memory()
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Settings {
    crate::store::load_settings(&app)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    crate::store::save_settings(&app, &settings)
}

#[tauri::command]
pub async fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    crate::store::set_launch_at_login(enabled)
}

#[tauri::command]
pub async fn is_launch_at_login() -> Result<bool, String> {
    crate::store::is_launch_at_login()
}

/// 向 Swift sidecar 发送一行 JSON 并读取一行 JSON 响应。
fn sidecar_call(req: serde_json::Value, connect_ms: u64, read_ms: u64) -> Result<serde_json::Value, String> {
    let addr: SocketAddr = "127.0.0.1:9876".parse().map_err(|_| "地址错误")?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(connect_ms))
        .map_err(|_| "sidecar 未运行".to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(read_ms))).ok();
    stream.set_write_timeout(Some(Duration::from_millis(2000))).ok();
    let payload = req.to_string() + "\n";
    stream.write_all(payload.as_bytes()).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|e| e.to_string())
}

/// 探测 Swift sidecar 健康状态（localhost:9876，TCP/JSON 行协议）。
///
/// 关键：先用 `ping` 判定「守护进程是否存活」——ping 不触碰 IOBluetooth，
/// 不会因蓝牙权限 / bluetoothd XPC 在 macOS 27 等环境下挂起而超时；
/// 再用 `bluetooth:status` 单独取蓝牙开关状态（独立连接、独立超时，互不阻塞）。
/// 这样即使蓝牙探测变慢/失败，面板「原生守护」仍正确显示「运行中」，
/// 而非把蓝牙问题误判成「守护未运行」。
#[tauri::command]
pub async fn sidecar_status() -> SidecarStatus {
    // 1) 存活判定：ping（不碰 IOBluetooth，绝不会挂起）
    let alive = match sidecar_call(json!({ "cmd": "ping" }), 400, 1500) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("sidecar ping failed: {}", e);
            false
        }
    };
    if !alive {
        return SidecarStatus { running: false, bluetooth: None };
    }
    // 2) 蓝牙状态：独立探测，失败仅置空，不影响「运行中」判定
    let bluetooth = match sidecar_call(json!({ "cmd": "bluetooth:status" }), 400, 2500) {
        Ok(v) => v.get("power").and_then(|x| x.as_bool()),
        Err(_) => None,
    };
    SidecarStatus { running: true, bluetooth }
}


// ---------------- 全局快捷键 ----------------

/// 保存并立即应用绑定。必须在阻塞线程池里执行：tauri-plugin-global-shortcut
/// 在 macOS 上会把注册/注销 dispatch 到主线程并同步等待，若在主线程调用会死锁。
#[tauri::command]
pub async fn set_shortcut_bindings(
    app: AppHandle,
    bindings: HashMap<String, String>,
) -> Result<Vec<ShortcutReport>, String> {
    let mut s = crate::store::load_settings(&app);
    s.shortcut_bindings = bindings.clone();
    crate::store::save_settings(&app, &s)?;
    let handle = app.clone();
    let reports = tauri::async_runtime::spawn_blocking(move || {
        crate::shortcuts::apply_bindings(&handle, &bindings)
    })
    .await
    .map_err(|e| format!("快捷键注册任务失败：{}", e))?;
    Ok(reports)
}

// ---------------- 在线更新 ----------------

/// 返回更新能力配置：是否已在构建期配置了更新服务（当前无线上地址 → 默认禁用）。
/// enabled 由编译期环境变量 SWITCHKIT_UPDATE_ENDPOINTS 决定：build-release.sh 在
/// 接入真实更新服务器时会导出该变量，option_env! 将其固化进二进制。
#[tauri::command]
pub fn get_update_config(app: AppHandle) -> UpdateConfig {
    let enabled = option_env!("SWITCHKIT_UPDATE_ENDPOINTS")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    UpdateConfig {
        enabled,
        current_version: app.package_info().version.to_string(),
    }
}
