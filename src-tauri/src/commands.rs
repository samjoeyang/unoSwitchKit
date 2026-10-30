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

fn is_desktop_hidden() -> bool {
    // CreateDesktop = false 表示隐藏桌面图标；不存在视为显示
    match util::run("defaults", &["read", "com.apple.finder", "CreateDesktop"]) {
        Ok(out) => out.trim() == "0",
        Err(_) => false,
    }
}

fn trash_path() -> std::path::PathBuf {
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
    let trash = trash_path();
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

fn parse_memory() -> Result<MemoryStat, String> {
    let page_size: u64 = 4096;
    let vm = util::run("vm_stat", &[])?;
    let mut free = 0u64;
    let mut active = 0u64;
    let mut inactive = 0u64;
    let mut speculative = 0u64;
    let mut wired = 0u64;
    let mut compressed = 0u64;
    for line in vm.lines() {
        let v = line.split_once(':').map(|(_, r)| r).unwrap_or("");
        if line.starts_with("Pages free") {
            free = parse_pages(v);
        } else if line.starts_with("Pages active") {
            active = parse_pages(v);
        } else if line.starts_with("Pages inactive") {
            inactive = parse_pages(v);
        } else if line.starts_with("Pages speculative") {
            speculative = parse_pages(v);
        } else if line.starts_with("Pages wired down") {
            wired = parse_pages(v);
        } else if line.starts_with("Pages occupied by compressor") {
            compressed = parse_pages(v);
        }
    }
    let total_out = util::run("sysctl", &["-n", "hw.memsize"])?;
    let total_bytes: u64 = total_out
        .trim()
        .parse()
        .map_err(|_| "解析总内存失败".to_string())?;
    let free_mb = (free + speculative) * page_size / 1_048_576;
    let used_mb = (active + inactive + wired + compressed) * page_size / 1_048_576;
    Ok(MemoryStat {
        free_mb,
        used_mb,
        total_mb: total_bytes / 1_048_576,
        compressed_mb: compressed * page_size / 1_048_576,
    })
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

#[tauri::command]
pub async fn toggle_dark_mode() -> Result<bool, String> {
    let next = !is_dark_mode();
    let script = format!(
        "tell application \"System Events\" to tell appearance preferences to set dark mode to {}",
        if next { "true" } else { "false" }
    );
    util::run_osascript(&script)?;
    Ok(next)
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
    let trash = trash_path();
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

/// 退出 SwitchKit（守护进程独立运行，不受影响）。
#[tauri::command]
pub fn quit_app(app: AppHandle) {
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
pub async fn flush_dns() -> Result<(), String> {
    crate::privilege::run_as_root("dscacheutil -flushcache; killall -HUP mDNSResponder")?;
    Ok(())
}

#[tauri::command]
pub async fn preview_cache_cleanup() -> CachePreview {
    let items = crate::cache_whitelist::preview();
    let total_bytes: u64 = items.iter().map(|i| i.size_bytes).sum();
    CachePreview {
        items,
        total_bytes,
    }
}

#[tauri::command]
pub async fn clear_cache() -> Result<u64, String> {
    crate::cache_whitelist::clear()
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
#[tauri::command]
pub async fn sidecar_status() -> SidecarStatus {
    match sidecar_call(json!({ "cmd": "bluetooth:status" }), 400, 800) {
        Ok(v) => SidecarStatus {
            running: true,
            bluetooth: v.get("power").and_then(|x| x.as_bool()),
        },
        Err(_) => SidecarStatus { running: false, bluetooth: None },
    }
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
