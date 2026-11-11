//! 权限自检与「换包后重新授权」引导。
//!
//! 背景：本项目长期使用 ad-hoc 签名、不申请 Developer ID / 不做公证，
//! 并接受「每次换包都要重新授权」。ad-hoc 每次构建都产生新的 cdhash，
//! TCC 把新包当成一个新 App，此前授予的隐私权限全部静默失效；系统设置里
//! 的开关仍显示「开」，而且通常不会重新弹窗（幽灵授权）。
//!
//! 因此这里做两件事：
//! 1. 用 `codesign -dv` 读取当前可执行文件的 CDHash，与上次记录比对，
//!    可靠地判断「包被替换了」（OTA / 手动覆盖 / 重新下载都适用，
//!    relaunch 之后无需额外埋点）；
//! 2. 提供逐项权限自检、一键复制 `tccutil reset` 命令、打开系统设置的入口，
//!    把「用户不知道该做什么」变成可操作的流程。
//!
//! 注意：探针分两类。无副作用探针（CGPreflight / 完全磁盘访问目录）可在
//! 启动时跑；AppleEvents 探针会触发系统授权弹窗，只在用户主动点「检查权限」
//! 时执行，并带超时，避免面板卡死。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const BUNDLE_ID: &str = "com.switchkit.app";
pub const RESET_COMMAND: &str = "tccutil reset All com.switchkit.app";

const PRIVACY_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation";
const PRIVACY_FALLBACK: &str = "x-apple.systempreferences:com.apple.preference.security";

// Apple 官方推荐的屏幕录制预检（macOS 11+ 才有实际实现，本项目基线 12.7.6）。
// 只查询、不申请权限，不会触发弹窗。
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

/// status: "granted" | "denied" | "unknown"
#[derive(Serialize, Clone)]
pub struct PermissionItem {
    pub key: String,
    pub label: String,
    pub status: String,
    /// 当前版本是否实际使用该权限；false 表示预留项，不参与告警。
    pub required: bool,
    pub detail: String,
    /// 对应的 tccutil service 名（大小写敏感）。
    pub service: String,
}

#[derive(Serialize)]
pub struct PermissionState {
    /// 当前签名哈希与上次记录不一致 → 刚换过包，需要重新授权。
    pub app_changed: bool,
    /// 当前 CDHash 前 8 位，仅用于诊断展示。
    pub cdhash: String,
    pub bundle_id: String,
    pub reset_command: String,
    /// 是否执行过主动探测（AppleEvents 探针会弹窗，只在用户点击时跑）。
    pub probed: bool,
    pub items: Vec<PermissionItem>,
}

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    last_cdhash: String,
    acknowledged_at: u64,
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("permission-state.json"))
}

fn load_state(app: &AppHandle) -> StateFile {
    let Some(p) = state_path(app) else {
        return StateFile::default();
    };
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<StateFile>(&s).ok())
        .unwrap_or_default()
}

fn save_state(app: &AppHandle, cdhash: &str) {
    let Some(p) = state_path(app) else {
        return;
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let state = StateFile {
        last_cdhash: cdhash.to_string(),
        acknowledged_at: ts,
    };
    if let Ok(text) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(&p, text);
    }
}

/// 读取当前可执行文件的 CDHash。ad-hoc 签名每次重建都会变，是判断
/// 「用户换了包」最可靠的信号。取不到时返回 None（如开发模式未签名二进制）。
fn current_cdhash() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let out = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(&exe)
        .output()
        .ok()?;
    // codesign -dv 的诊断信息写在 stderr。
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.lines()
        .find_map(|l| l.trim().strip_prefix("CDHash=").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
}

fn item(
    key: &str,
    label: &str,
    required: bool,
    service: &str,
    detail: &str,
) -> PermissionItem {
    PermissionItem {
        key: key.to_string(),
        label: label.to_string(),
        status: "unknown".to_string(),
        required,
        detail: detail.to_string(),
        service: service.to_string(),
    }
}

fn base_items() -> Vec<PermissionItem> {
    vec![
        item(
            "automation_finder",
            "自动化（Finder）",
            true,
            "AppleEvents",
            "清空废纸篓、废纸篓计数；点「检查权限」时会触发一次系统授权弹窗",
        ),
        item(
            "automation_system_events",
            "自动化（System Events）",
            true,
            "AppleEvents",
            "深色模式切换、屏幕清洁、关闭所有应用",
        ),
        item(
            "full_disk_access",
            "完全磁盘访问",
            true,
            "SystemPolicyAllFiles",
            "清理 Safari / Mail 等受 TCC 保护的缓存；该权限不自动弹窗，需在系统设置里手动添加",
        ),
        item(
            "files_folders",
            "文件和文件夹（桌面 / 文稿 / 下载）",
            true,
            "SystemPolicyDesktopFolder",
            "大文件扫描、应用占用统计；由系统在实际访问时弹窗，不做主动探测以免面板卡住",
        ),
        item(
            "bluetooth",
            "蓝牙",
            true,
            "BluetoothAlways",
            "设备列表 / 连接 / 断开；由 Swift 守护在首次扫描时弹窗",
        ),
        item(
            "screen_capture",
            "屏幕录制（预留）",
            false,
            "ScreenCapture",
            "当前版本未使用；后续采集类功能会用到。CGPreflight 在刚授权后可能返回旧值，必要时重启 App",
        ),
    ]
}

/// 完全磁盘访问类目录：未授权时内核直接返回 EPERM，不会弹窗、不会挂起。
fn probe_full_disk_access() -> String {
    let home = crate::util::home_dir();
    let mut checked = 0usize;
    let mut denied = 0usize;
    for rel in [
        "Library/Safari",
        "Library/Mail",
        "Library/Containers/com.apple.Safari",
    ] {
        match fs::read_dir(home.join(rel)) {
            Ok(_) => checked += 1,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                checked += 1;
                denied += 1;
            }
            Err(_) => {} // 目录不存在等，跳过
        }
    }
    if checked == 0 {
        "unknown".to_string()
    } else if denied > 0 {
        "denied".to_string()
    } else {
        "granted".to_string()
    }
}

/// 无副作用探针，可在启动时执行。
fn passive_probe(items: &mut [PermissionItem]) {
    for it in items.iter_mut() {
        match it.key.as_str() {
            "screen_capture" => {
                // CGPreflightScreenCaptureAccess 只查询不申请，且 11+ 可用。
                it.status = if unsafe { CGPreflightScreenCaptureAccess() } {
                    "granted"
                } else {
                    "denied"
                }
                .to_string();
            }
            "full_disk_access" => it.status = probe_full_disk_access(),
            _ => {}
        }
    }
}

/// 带超时执行 osascript。授权弹窗等待用户响应时 AppleEvent 会一直挂起，
/// 3 秒后强杀子进程，避免面板卡死。`with timeout` 是 AppleScript 层的兜底。
fn osascript_with_timeout(script: &str, timeout: Duration) -> Option<std::process::Output> {
    let child = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => Some(out),
        Ok(Err(_)) => None,
        Err(_) => {
            let _ = Command::new("/bin/kill")
                .args(["-9", &pid.to_string()])
                .output();
            None
        }
    }
}

fn probe_apple_events(script: &str) -> String {
    let wrapped = format!("with timeout of 2 seconds\n{}\nend timeout", script);
    match osascript_with_timeout(&wrapped, Duration::from_secs(3)) {
        Some(out) if out.status.success() => "granted".to_string(),
        Some(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            // -1743 = 未授权；-1712 = AppleEvent 超时（多半卡在授权弹窗）。
            if err.contains("-1743") || err.contains("-1712") || err.contains("Not authorized") {
                "denied".to_string()
            } else {
                "unknown".to_string()
            }
        }
        None => "unknown".to_string(),
    }
}

/// 主动探针，仅在用户点「检查权限」时执行（可能触发系统授权弹窗）。
fn active_probe(items: &mut [PermissionItem]) {
    for it in items.iter_mut() {
        match it.key.as_str() {
            "automation_finder" => {
                it.status = probe_apple_events(
                    "tell application \"Finder\" to return name of startup disk",
                );
            }
            "automation_system_events" => {
                it.status = probe_apple_events(
                    "tell application \"System Events\" to return name of first application process",
                );
            }
            _ => {}
        }
    }
}

fn build_state(app: &AppHandle, probed: bool) -> PermissionState {
    let cur = current_cdhash();
    let saved = load_state(app);
    let app_changed = match cur.as_deref() {
        // 首次运行（无历史记录）不算「换包」：那时本来就会走首次授权弹窗。
        // 但必须立刻建档，否则用户从未点过「已处理」时，之后的 OTA 换包
        // 会因为两边都为空而检测不到。
        Some(c) if saved.last_cdhash.is_empty() => {
            save_state(app, c);
            false
        }
        Some(c) => saved.last_cdhash != c,
        None => false,
    };
    let mut items = base_items();
    passive_probe(&mut items);
    if probed {
        active_probe(&mut items);
    }
    PermissionState {
        app_changed,
        cdhash: cur.map(|c| c.chars().take(8).collect()).unwrap_or_default(),
        bundle_id: BUNDLE_ID.to_string(),
        reset_command: RESET_COMMAND.to_string(),
        probed,
        items,
    }
}

/// 面板启动 / 重新显示时调用：只做零副作用探测。
#[tauri::command]
pub fn get_permission_state(app: AppHandle) -> PermissionState {
    build_state(&app, false)
}

/// 用户点「检查权限」时调用：会尝试触发 AppleEvents 授权弹窗，带超时。
#[tauri::command]
pub fn refresh_permissions(app: AppHandle) -> PermissionState {
    build_state(&app, true)
}

/// 用户完成重新授权后点「已处理」，把当前签名记为已确认，消除提示。
#[tauri::command]
pub fn acknowledge_permission_change(app: AppHandle) -> bool {
    match current_cdhash() {
        Some(c) => {
            save_state(&app, &c);
            true
        }
        None => false,
    }
}

/// 把重置命令写进剪贴板（Tauri WKWebView 的 clipboard API 不可靠，走 pbcopy）。
#[tauri::command]
pub fn copy_reset_command() -> Result<(), String> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 pbcopy 失败: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(RESET_COMMAND.as_bytes())
            .map_err(|e| format!("写入剪贴板失败: {}", e))?;
    }
    child
        .wait()
        .map_err(|e| format!("pbcopy 未正常退出: {}", e))?;
    Ok(())
}

/// 打开「系统设置 → 隐私与安全性 → 自动化」；旧 URL 在新系统上失效时回退到总页。
#[tauri::command]
pub fn open_privacy_settings() -> Result<(), String> {
    let ok = Command::new("/usr/bin/open")
        .arg(PRIVACY_PANE)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        return Ok(());
    }
    Command::new("/usr/bin/open")
        .arg(PRIVACY_FALLBACK)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        .then_some(())
        .ok_or_else(|| "无法打开系统设置".to_string())
}
