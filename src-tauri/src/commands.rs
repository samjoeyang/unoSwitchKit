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

fn is_prevent_sleep(state: State<AppState>) -> bool {
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

#[tauri::command]
pub fn get_toggle_states(state: State<AppState>) -> ToggleStates {
    ToggleStates {
        dark_mode: is_dark_mode(),
        desktop_hidden: is_desktop_hidden(),
        prevent_sleep: is_prevent_sleep(state),
        trash_count: trash_count(),
    }
}

#[tauri::command]
pub fn toggle_dark_mode() -> Result<bool, String> {
    let next = !is_dark_mode();
    let script = format!(
        "tell application \"System Events\" to tell appearance preferences to set dark mode to {}",
        if next { "true" } else { "false" }
    );
    util::run_osascript(&script)?;
    Ok(next)
}

#[tauri::command]
pub fn toggle_hide_desktop() -> Result<bool, String> {
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
pub fn toggle_prevent_sleep(state: State<AppState>) -> Result<bool, String> {
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
pub fn empty_trash() -> Result<u32, String> {
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
pub fn release_memory() -> Result<MemoryComparison, String> {
    let before = parse_memory()?;
    crate::privilege::run_as_root("purge")?;
    let after = parse_memory()?;
    Ok(MemoryComparison { before, after })
}

#[tauri::command]
pub fn flush_dns() -> Result<(), String> {
    crate::privilege::run_as_root("dscacheutil -flushcache; killall -HUP mDNSResponder")?;
    Ok(())
}

#[tauri::command]
pub fn preview_cache_cleanup() -> CachePreview {
    let items = crate::cache_whitelist::preview();
    let total_bytes: u64 = items.iter().map(|i| i.size_bytes).sum();
    CachePreview {
        items,
        total_bytes,
    }
}

#[tauri::command]
pub fn clear_cache() -> Result<u64, String> {
    crate::cache_whitelist::clear()
}

#[tauri::command]
pub fn get_memory_stats() -> Result<MemoryStat, String> {
    parse_memory()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    crate::store::load_settings(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    crate::store::save_settings(&app, &settings)
}

#[tauri::command]
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    crate::store::set_launch_at_login(enabled)
}

#[tauri::command]
pub fn is_launch_at_login() -> Result<bool, String> {
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
pub fn sidecar_status() -> SidecarStatus {
    match sidecar_call(json!({ "cmd": "bluetooth:status" }), 400, 800) {
        Ok(v) => SidecarStatus {
            running: true,
            bluetooth: v.get("power").and_then(|x| x.as_bool()),
        },
        Err(_) => SidecarStatus { running: false, bluetooth: None },
    }
}

/// 菜单栏图标管理状态：枚举 + 合并白名单 + 推送巡检策略。
#[tauri::command]
pub fn menubar_state(app: AppHandle) -> MenuBarState {
    let mut settings = crate::store::load_settings(&app);

    let resp = sidecar_call(json!({ "cmd": "menubar:list" }), 600, 8000);
    let running = resp.is_ok();
    let ax_trusted = resp.as_ref()
        .ok()
        .and_then(|v| v.get("axTrusted"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);

    let mut items: Vec<MenuBarItemState> = Vec::new();
    if let Ok(v) = &resp {
        if let Some(arr) = v.get("items").and_then(|x| x.as_array()) {
            for it in arr {
                let id = it.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if id.is_empty() {
                    continue;
                }
                let x = it.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let y = it.get("y").and_then(|x| x.as_f64()).unwrap_or(0.0);
                let visible = it.get("visible").and_then(|x| x.as_bool()).unwrap_or(false);
                // 记录可见项的原始位置（隐藏后恢复用），持久化
                if visible {
                    settings
                        .menubar_positions
                        .entry(id.clone())
                        .or_insert([x, y]);
                }
                items.push(MenuBarItemState {
                    show: settings.menubar_visible.contains(&id),
                    visible,
                    id,
                    owner: it.get("owner").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    title: it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                });
            }
        }
    }

    // 推送巡检策略（守护进程按此持续执行）
    if running {
        let visible_ids: Vec<String> = items.iter().filter(|i| i.show).map(|i| i.id.clone()).collect();
        let _ = sidecar_call(
            json!({
                "cmd": "menubar:apply",
                "ownPid": std::process::id(),
                "managed": settings.menubar_managed,
                "visibleIds": visible_ids,
                "positions": settings.menubar_positions,
            }),
            600,
            8000,
        );
    }

    let _ = crate::store::save_settings(&app, &settings);
    MenuBarState { ax_trusted, running, managed: settings.menubar_managed, items }
}

/// 点亮 / 收起某个菜单栏图标（更新白名单；巡检由 menubar_state 推送生效）。
#[tauri::command]
pub fn menubar_set_visible(app: AppHandle, id: String, show: bool) -> Result<(), String> {
    let mut settings = crate::store::load_settings(&app);
    if show {
        if !settings.menubar_visible.contains(&id) {
            settings.menubar_visible.push(id);
        }
    } else {
        settings.menubar_visible.retain(|x| x != &id);
    }
    crate::store::save_settings(&app, &settings)
}

/// 菜单栏管理总开关（关闭 = 全部恢复显示并停止巡检）。
#[tauri::command]
pub fn menubar_set_managed(app: AppHandle, managed: bool) -> Result<(), String> {
    let mut settings = crate::store::load_settings(&app);
    settings.menubar_managed = managed;
    crate::store::save_settings(&app, &settings)
}
