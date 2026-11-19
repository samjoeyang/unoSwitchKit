//! 原生守护的「启动诊断日志」。
//!
//! 与另外两份日志的分工：
//! - `switchkit-rust.log`：人读的生命周期流水（旧格式，保留兼容）；
//! - `switchkit-daemon.log`：Swift 守护 stderr，包含逐请求明细，体积会持续增长；
//! - **本文件**：一次启动/自愈的完整决策链，结构化 key=value，单行可解析，
//!   带 session 便于跨进程重启关联，且有体积上限（不会像 daemon.log 那样膨胀）。
//!
//! 设计目标：面板「原生守护」变灰后，只需回传本文件即可还原失败链：
//! 何时触发、候选二进制的架构/签名/隔离状态、spawn errno、子进程退出码、
//! 端口/ping 判活结果、看门狗决策、以及系统策略（AMFI/Gatekeeper/TCC）拒绝记录。
//!
//! 本模块只用 std，不依赖 Tauri；可在任意线程调用，写文件用 append + 进程内互斥。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 单次 App 会话 ID（unix 秒 + pid），用于把跨重启的日志分段。
static SESSION: AtomicU64 = AtomicU64::new(0);
/// 前端状态探测失败日志的节流时间戳（unix 秒），避免 5s 轮询刷屏。
static LAST_STATUS_FAIL: AtomicU64 = AtomicU64::new(0);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

const MAX_BYTES: u64 = 512 * 1024;
const KEEP_BYTES: u64 = 256 * 1024;

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_iso() -> String {
    // 不引入 chrono：用 /bin/date 拿带时区的可读时间，失败时退回 unix 秒。
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
    format!("{}", unix_secs())
}

pub fn log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    Path::new(&home)
        .join("Library")
        .join("Logs")
        .join("switchkit-daemon-startup.log")
}

/// 值转义：保证一行一个事件，日志可直接按空格/引号解析。
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// 体积上限：超过 512KB 时保留最后 256KB。诊断只需最近若干次启动。
fn rotate_if_needed(path: &Path) {
    let ok = std::fs::metadata(path)
        .map(|m| m.len() > MAX_BYTES)
        .unwrap_or(false);
    if !ok {
        return;
    }
    if let Ok(bytes) = std::fs::read(path) {
        let skip = bytes.len().saturating_sub(KEEP_BYTES as usize);
        if skip > 0 {
            let _ = std::fs::write(path, &bytes[skip..]);
        }
    }
}

/// 写一条结构化事件。fields 中的值由调用方给出，统一转义。
pub fn log(event: &str, fields: &[(&str, String)]) {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_if_needed(&path);
    let session = SESSION.load(Ordering::SeqCst);
    let mut line = format!(
        "[{}] session={:x} event={}",
        now_iso(),
        session,
        event
    );
    for (k, v) in fields {
        line.push(' ');
        line.push_str(k);
        line.push_str("=\"");
        line.push_str(&esc(v));
        line.push('"');
    }
    line.push('\n');
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// App 启动时调用一次，写入环境指纹。
pub fn init_session() {
    let s = unix_secs() ^ (std::process::id() as u64).wrapping_mul(0x9e3779b97f4a7c15);
    SESSION.store(s, Ordering::SeqCst);
    let os = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    log(
        "session_begin",
        &[
            ("app_version", env!("CARGO_PKG_VERSION").to_string()),
            ("os_version", os),
            ("arch", std::env::consts::ARCH.to_string()),
            ("app_pid", std::process::id().to_string()),
            ("log_file", log_path().to_string_lossy().to_string()),
        ],
    );
}

/// 记录候选守护二进制的静态属性。这些是 Silicon 上最常断的几环：
/// 架构、ad-hoc 签名、CDHash（换包即变）、quarantine、可执行权限。
pub fn log_binary_info(path: &std::path::Path) {
    let p = path.to_string_lossy().to_string();
    let lipo = std::process::Command::new("/usr/bin/lipo")
        .args(["-archs", &p])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let sign = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=2", &p])
        .output()
        .ok()
        .map(|o| {
            let t = String::from_utf8_lossy(&o.stderr).to_string()
                + &String::from_utf8_lossy(&o.stdout);
            let mut parts = Vec::new();
            for line in t.lines() {
                let l = line.trim();
                if l.starts_with("Signature=")
                    || l.starts_with("CDHash=")
                    || l.starts_with("TeamIdentifier=")
                    || l.starts_with("Identifier=")
                    || l.contains("not signed at all")
                {
                    parts.push(l.to_string());
                }
            }
            parts.join("; ")
        })
        .unwrap_or_default();
    let xattr = std::process::Command::new("/usr/bin/xattr")
        .arg("-l")
        .arg(&p)
        .output()
        .ok()
        .map(|o| {
            let t = String::from_utf8_lossy(&o.stdout).to_string();
            if t.to_lowercase().contains("quarantine") {
                "PRESENT".to_string()
            } else {
                "none".to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    let stat = std::process::Command::new("/usr/bin/stat")
        .args(["-f", "%z %Sp", &p])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    log(
        "binary_info",
        &[
            ("path", p),
            ("arch", lipo),
            ("codesign", sign),
            ("quarantine", xattr),
            ("size_mode", stat),
        ],
    );
}

/// 把本次 spawn 之前未记录的守护 stderr 增量抄进诊断日志。
/// 守护若在 Swift 第一行前被拦，daemon.log 里没有任何 launch 行——
/// 这段增量就是唯一的 exec 期证据（bind 失败、FATAL、签名错误等）。
pub fn log_stderr_since(path: &std::path::Path, offset: u64) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() <= offset {
            return;
        }
    }
    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::{Read, Seek, SeekFrom};
        if f.seek(SeekFrom::Start(offset)).is_ok() {
            let max = 4096usize;
            let mut buf = vec![0u8; max];
            let n = f.read(&mut buf).unwrap_or(0);
            if n > 0 {
                log(
                    "daemon_stderr",
                    &[("tail", String::from_utf8_lossy(&buf[..n]).to_string())],
                );
            }
        }
    }
}

/// 失败时抓系统策略日志。macOS 27 上受限 entitlement / 未签名 helper
/// 的拒绝发生在 taskgated-helper、amfid、syspolicyd、tccd，不在应用日志里。
/// 只在实际失败/超时时调用，避免常规启动拖慢面板。
pub fn capture_syspolicy(prefix: &str) {
    let out = std::process::Command::new("/usr/bin/log")
        .args([
            "show",
            "--last",
            "3m",
            "--style",
            "compact",
            "--predicate",
            "process == \"amfid\" || process == \"syspolicyd\" || process == \"taskgated-helper\" || process == \"tccd\"",
        ])
        .output();
    let text = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return,
    };
    let hits: Vec<&str> = text
        .lines()
        .filter(|l| {
            let s = l.to_lowercase();
            s.contains("switchkit") || s.contains("daemon") || s.contains("amfi")
                || s.contains("deny") || s.contains("reject") || s.contains("entitlement")
        })
        .rev()
        .take(20)
        .collect();
    if hits.is_empty() {
        log("syspolicy", &[("phase", prefix.to_string()), ("hits", "0".to_string())]);
        return;
    }
    for line in hits.iter().rev() {
        log(
            "syspolicy",
            &[
                ("phase", prefix.to_string()),
                ("line", (*line).to_string()),
            ],
        );
    }
}

/// 节流：同一类高频事件（前端轮询失败）最多每 interval_secs 秒记一次。
pub fn should_log_throttled(interval_secs: u64) -> bool {
    let now = unix_secs();
    let last = LAST_STATUS_FAIL.load(Ordering::SeqCst);
    if now.saturating_sub(last) >= interval_secs {
        LAST_STATUS_FAIL.store(now, Ordering::SeqCst);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_keeps_one_event_per_line() {
        assert_eq!(esc("a\"b\nc\\d\te"), "a\\\"b\\nc\\\\d\\te");
    }

    #[test]
    fn log_path_under_logs_dir() {
        let p = log_path();
        assert!(p.ends_with("switchkit-daemon-startup.log"));
        assert!(p.to_string_lossy().contains("Logs"));
    }
}
