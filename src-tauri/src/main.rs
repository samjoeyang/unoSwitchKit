#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod auth;
mod balance;
mod daemon_log;
mod cleanup;
mod commands;
mod sysmon;
mod models;
mod permissions;
mod privilege;
mod shortcuts;
mod store;
mod tray;
mod util;

use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::Manager;

// ---- 原生守护全生命周期日志（诊断用） ----
// GUI 启动（双击 / LaunchPad / 登录项）的 App 进程没有 tty，stderr 被丢弃，
// 因此必须把生命周期事件落盘到 ~/Library/Logs/，才能在用户真机上事后分析
// 「守护为何没起来」。Swift 侧见 src-tauri/swift/main.swift 的同名日志。
use std::io::Write;

/// 守护启动串行锁：setup 线程与看门狗线程不得同时 spawn，
/// 否则两边各自 kill_port_holder 会互相杀掉刚 listen 的守护。
static SPAWN_LOCK: Mutex<()> = Mutex::new(());
/// spawn 进行中（可能长达 30s 等系统策略评估）。看门狗据此跳过本轮，
/// 否则它会在 SPAWN_LOCK 上排队，锁一释放就再拉一个守护抢 9876。
static SPAWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
/// 退出时置位，阻止看门狗在退出清理后又把守护拉起来。
static WATCHDOG_STOP: AtomicBool = AtomicBool::new(false);
/// 最近一次 spawn 完成的 unix 秒（0 = 首轮 spawn 尚未结束）。看门狗据此冷却。
static LAST_SPAWN_DONE: AtomicU64 = AtomicU64::new(0);
/// 上次「已拉起但 30s 内还没响应 ping」的守护 PID。下次自愈前先精确回收它，
/// 避免在 macOS 27 慢启动场景下每轮留下一个不监听端口的僵尸守护。
static PENDING_DAEMON_PID: AtomicU32 = AtomicU32::new(0);

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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

/// PID 是否确属本项目守护。`kill -9` 按 PID 操作，必须防 PID 复用误杀无关进程。
fn is_our_daemon_pid(pid: u32) -> bool {
    std::process::Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("switchkit-daemon"))
        .unwrap_or(false)
}

/// 回收上次「已拉起但未就绪」的守护：macOS 27 上 AMFI/Gatekeeper 评估可能
/// 远超就绪窗口，此时进程仍活着、只是慢；不能 kill，但下一轮自愈前若仍未
/// 监听端口，必须精确回收，否则每轮都会遗留一个不干活的子进程。
fn reap_stale_pending_daemon() {
    let pid = PENDING_DAEMON_PID.swap(0, Ordering::SeqCst);
    if pid == 0 || !is_process_alive(pid) || !is_our_daemon_pid(pid) {
        return;
    }
    if port_listening() {
        // 可能刚好就绪了，交给入口的 daemon_reachable 判定，不在这里杀。
        return;
    }
    lifecycle_log(&format!(
        "reap_stale_pending_daemon: kill -9 pid {} (still alive, never listened)",
        pid
    ));
    daemon_log::log(
        "stale_daemon_reaped",
        &[("pid", pid.to_string()), ("reason", "alive_but_never_listened".to_string())],
    );
    let _ = std::process::Command::new("/bin/kill")
        .args(["-9", &pid.to_string()])
        .status();
    for _ in 0..20 {
        if !is_process_alive(pid) {
            lifecycle_log(&format!("reap_stale_pending_daemon: pid {} gone", pid));
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    lifecycle_log(&format!(
        "reap_stale_pending_daemon: WARN pid {} still alive after ~2s",
        pid
    ));
    daemon_log::log(
        "stale_daemon_reap_failed",
        &[("pid", pid.to_string())],
    );
}

/// 端口是否正在监听（裸 TCP connect，仅用于快速判定）。
fn port_listening() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
}

/// 守护是否「真正可达」：连上并收到 ping 的 JSON 响应。
/// 比裸 TCP connect 严格——accept 线程活着但守护假死/正在退出时 connect 仍成功，
/// 据此判断「守护健康」会漏掉自愈；据此判断 reuse 会把将死进程当成活守护。
fn daemon_reachable(connect_ms: u64, read_ms: u64) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let addr: std::net::SocketAddr = match "127.0.0.1:9876".parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(connect_ms)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(read_ms)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    if stream.write_all(b"{\"cmd\":\"ping\"}\n").is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(_) => line.contains("\"ok\"") && line.contains("true"),
        Err(_) => false,
    }
}

/// 全局可变状态：prevent-sleep 的 caffeinate 子进程句柄 + 屏幕清洁状态 + 系统监控。
pub struct AppState {
    pub caffeinate: Mutex<Option<Child>>,
    /// 屏幕清洁是否处于「已收起全部前台窗口」状态（本应用维护）。
    pub screen_cleaned: Mutex<bool>,
    /// 系统实时监控（CPU/内存/网络），跨命令调用保留同一 sysinfo 实例。
    pub sysmon: Mutex<crate::sysmon::SystemMonitor>,
}

/// 清掉占用 9876 端口的进程（best-effort）。用于两处：
/// ① 启动时发现端口被旧版本残留守护占用——旧版无看门狗、且曾有过
///    `bluetooth:status` 挂起 bug（见 06faba6），必须换成本次构建的健康守护，
///    否则状态探测会超时、面板显示「原生守护未运行」；
/// ② 退出兜底（见 commands::shutdown_sidecar）。
pub fn kill_port_holder() {
    // 必须用绝对路径：GUI 启动（双击 / LaunchPad / 登录项）时进程 PATH 仅含
    // /usr/bin:/bin，裸 `lsof` 会 command not found，导致清不掉端口占用。
    let out = std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "/usr/sbin/lsof -nP -tiTCP:9876 -sTCP:LISTEN 2>/dev/null | while read -r p; do kill -9 \"$p\" 2>/dev/null; echo \"$p\"; done",
        ])
        .output();
    // 关键：kill -9 是异步的，内核回收进程 + 释放监听端口需要时间。
    // 旧实现发完 kill 立刻 spawn，新守护 bind 时旧 socket 尚未释放 → bind 失败；
    // 或反过来裸 TCP connect 探测到「将死未死」的旧守护仍 LISTEN，被误判为复用。
    // 这里轮询等待端口真正释放（最多 ~5s），再让调用方继续。
    let started = std::time::Instant::now();
    let killed = out
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if !killed.is_empty() {
        let pids = killed.replace('\n', ",");
        lifecycle_log(&format!(
            "kill_port_holder: killed pids [{}], waiting for 9876 to free",
            pids
        ));
        daemon_log::log(
            "kill_port_holder",
            &[("killed_pids", pids), ("action", "wait_for_release".to_string())],
        );
        for _ in 0..50 {
            if !port_listening() {
                let ms = started.elapsed().as_millis().to_string();
                lifecycle_log("kill_port_holder: 9876 freed");
                daemon_log::log(
                    "port_freed",
                    &[("elapsed_ms", ms)],
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        lifecycle_log("kill_port_holder: WARN 9876 still listening after ~5s");
        daemon_log::log(
            "port_release_failed",
            &[("elapsed_ms", started.elapsed().as_millis().to_string())],
        );
    }
}

/// 拉起 Swift 守护进程（蓝牙承载），独立于 App 生命周期常驻。
/// 端口已被占用（旧实例在跑）时直接跳过——守护自身带父进程看门狗，
/// 父进程退出即自我回收，无需本进程持有其句柄。
/// 全程向 switchkit-rust.log 写生命周期日志，便于真机事后定位。
/// 调用方：App 启动（后台线程）+ 守护看门狗自愈；两者由 SPAWN_LOCK 串行化。
fn spawn_sidecar(reason: &str) {
    lifecycle_log(&format!("spawn_sidecar: trigger={}", reason));
    daemon_log::log("spawn_trigger", &[("reason", reason.to_string())]);
    let _guard = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SPAWN_IN_PROGRESS.store(true, Ordering::SeqCst);
    spawn_sidecar_inner();
    LAST_SPAWN_DONE.store(unix_secs(), Ordering::SeqCst);
    SPAWN_IN_PROGRESS.store(false, Ordering::SeqCst);
}

/// 同时写人读日志与结构化诊断日志，避免两处口径不一致。
fn dlog(event: &str, detail: &str) {
    lifecycle_log(&format!("{}: {}", event, detail));
    daemon_log::log(event, &[("detail", detail.to_string())]);
}

fn spawn_sidecar_inner() {
    lifecycle_log("spawn_sidecar: enter");
    dlog("entry_check", "checking existing daemon before spawn");
    // 先复核守护是否真能响应 ping。看门狗在等待 SPAWN_LOCK 期间，首轮 spawn
    // 可能已完成且守护已就绪——此时直接往下走会 kill 掉健康守护再重拉，
    // 这正是「原生守护反复亮灭」的根因之一。
    if daemon_reachable(300, 1200) {
        lifecycle_log("spawn_sidecar: daemon ping OK on entry, reuse, skip spawn");
        dlog("conclusion", "cause=none; daemon already reachable, reuse");
        return;
    }
    dlog("entry_check", "ping failed; proceed to cleanup and spawn");
    // 上一轮「慢启动未完成」的残留：精确回收，避免 Apple Silicon 上反复 spawn
    // 累积多个不监听端口的子进程（它们不占 9876，kill_port_holder 清不到）。
    reap_stale_pending_daemon();
    // 升级/异常退出残留：清掉占用 9876 的旧守护，确保拉起的是本次构建的健康守护。
    // （此前 693dc59 回退时误删了 dffe193 的这步清理，导致升级后原生守护不亮。）
    kill_port_holder(); // 内部已等端口真正释放，避免 kill -9 异步竞态
    lifecycle_log("spawn_sidecar: kill_port_holder done");
    if port_listening() {
        lifecycle_log("spawn_sidecar: 9876 still occupied after kill, abort to avoid bind race");
        dlog("conclusion", "cause=port_busy; 9876 still listening after kill, aborted");
        return;
    }
    dlog("port_check", "9876 free; searching daemon binary");
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
            dlog("candidate_missing", &path.to_string_lossy());
            continue;
        }
        lifecycle_log(&format!(
            "spawn_sidecar: candidate exists: {}",
            path.display()
        ));
        daemon_log::log_binary_info(&path);
        // 关键（Apple Silicon 修复）：先清掉嵌套守护自身的隔离属性再 spawn。
        #[cfg(target_os = "macos")]
        strip_quarantine(&path);
        // 优先直接拉起 bundle 内守护（已清隔离，Apple Silicon 上 AMFI 放行）。
        // Pending = 进程已起但系统策略评估未完成；此时绝不能 kill，也不能再拉
        // 第二个守护去抢 9876 —— 交给看门狗继续观察即可。
        match try_spawn_daemon(&path) {
            SpawnResult::Ready => {
                lifecycle_log("spawn_sidecar: daemon up via bundle path, done");
                dlog("conclusion", "cause=none; daemon ready via bundle path");
                return;
            }
            SpawnResult::Pending(pid) => {
                lifecycle_log(&format!(
                    "spawn_sidecar: bundle daemon pid {} still initializing, defer to watchdog",
                    pid
                ));
                dlog(
                    "conclusion",
                    &format!("cause=slow_start; pid {} alive but not ready after 30s, kept for watchdog", pid),
                );
                return;
            }
            SpawnResult::Failed => {}
        }
        lifecycle_log("spawn_sidecar: bundle path spawn FAILED, fallback to Application Support");
        dlog("fallback_begin", "bundle path failed; trying Application Support copy");
        // bundle 内仍拉不起（极端环境），退回到 Application Support 副本。
        #[cfg(target_os = "macos")]
        {
            let dest = prepare_daemon_macos(&path);
            daemon_log::log_binary_info(&dest);
            match try_spawn_daemon(&dest) {
                SpawnResult::Ready => {
                    lifecycle_log("spawn_sidecar: daemon up via Application Support fallback, done");
                    dlog("conclusion", "cause=none; daemon ready via Application Support copy");
                    return;
                }
                SpawnResult::Pending(pid) => {
                    lifecycle_log(&format!(
                        "spawn_sidecar: fallback daemon pid {} still initializing, defer to watchdog",
                        pid
                    ));
                    dlog(
                        "conclusion",
                        &format!("cause=slow_start; fallback pid {} alive but not ready after 30s", pid),
                    );
                    return;
                }
                SpawnResult::Failed => {}
            }
            lifecycle_log("spawn_sidecar: Application Support fallback also FAILED");
        }
        dlog(
            "conclusion",
            "cause=spawn_or_exec_failed; see daemon_stderr/syspolicy events above",
        );
        daemon_log::capture_syspolicy("both_paths_failed");
        return;
    }
    lifecycle_log("spawn_sidecar: no candidate path found, daemon NOT launched");
    dlog("conclusion", "cause=binary_missing; no candidate daemon path exists");
}

/// 守护启动结果。`Pending` = 进程已拉起但系统策略评估未完成，不能再当失败 kill。
enum SpawnResult {
    Ready,
    Pending(u32),
    Failed,
}

/// spawn 守护进程并把它的 stderr 重定向到 daemon 日志文件，探测 ping 是否就绪。
/// 正常启动 <1s 即返回，不拖慢面板出现。
/// 关键（Apple Silicon / macOS 27）：ad-hoc 换包后首次 exec 会重做签名与
/// Gatekeeper 评估，可能卡在 Swift 第一行之前十几秒。旧实现固定 9s 超时后
/// `kill -9` 掉这个健康进程，看门狗再拉再超时 → 「原生守护反复不亮」。
/// 现在给 30s 窗口，且每轮检查子进程是否已退出；超时后仍活着则保留
/// （Pending），由看门狗继续确认，绝不误杀慢启动。
fn try_spawn_daemon(path: &std::path::Path) -> SpawnResult {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};

    // 记录 daemon.log 当前偏移量：失败后把本次 spawn 新增的 stderr 抄进诊断日志，
    // 这样即使守护第一行都没执行（被 AMFI 拦在 exec），也有可回传的证据。
    let log_file = daemon_log_path();
    let stderr_offset = std::fs::metadata(&log_file)
        .map(|m| m.len())
        .unwrap_or(0);
    // 守护 stderr 重定向到 daemon 日志：GUI 下守护 stderr 被丢弃，落盘后我们
    // 才能看到它 bind/权限阶段的报错（如被 AMFI 拦截、端口占用）。
    let mut cmd = std::process::Command::new(path);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(f) => {
            cmd.stderr(f);
        }
        Err(_) => {
            cmd.stderr(std::process::Stdio::null());
        }
    }
    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let detail = format!(
                "path={} error={} raw_os_error={:?}",
                path.display(),
                e,
                e.raw_os_error()
            );
            lifecycle_log(&format!("try_spawn_daemon: spawn FAILED {}", detail));
            daemon_log::log("spawn_error", &[("detail", detail)]);
            daemon_log::capture_syspolicy("spawn_error");
            return SpawnResult::Failed;
        }
    };
    let pid = child.id();
    lifecycle_log(&format!(
        "try_spawn_daemon: spawned pid {} ({})",
        pid,
        path.display()
    ));
    daemon_log::log(
        "spawn_ok",
        &[
            ("pid", pid.to_string()),
            ("path", path.to_string_lossy().to_string()),
        ],
    );
    let deadline = started + Duration::from_secs(30);
    let mut next_report = started + Duration::from_secs(10);
    loop {
        // ping 比裸 TCP connect 严格：LISTEN 但假死不算就绪。
        if daemon_reachable(50, 300) {
            let ms = started.elapsed().as_millis().to_string();
            lifecycle_log(&format!(
                "try_spawn_daemon: pid {} ping OK on 9876 after {}ms",
                pid, ms
            ));
            daemon_log::log(
                "ready",
                &[("pid", pid.to_string()), ("elapsed_ms", ms)],
            );
            PENDING_DAEMON_PID.store(0, Ordering::SeqCst);
            std::mem::forget(child); // 脱离句柄，由看门狗回收
            return SpawnResult::Ready;
        }
        // 子进程已退出就没必要再等；try_wait 同时完成回收，避免僵尸。
        if let Ok(Some(status)) = child.try_wait() {
            let ms = started.elapsed().as_millis().to_string();
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            #[cfg(unix)]
            let signal = status
                .signal()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "none".to_string());
            #[cfg(not(unix))]
            let signal = "n/a".to_string();
            lifecycle_log(&format!(
                "try_spawn_daemon: pid {} exited early: {}; stderr in daemon log",
                pid, status
            ));
            daemon_log::log(
                "child_exit",
                &[
                    ("pid", pid.to_string()),
                    ("elapsed_ms", ms),
                    ("exit_code", code),
                    ("signal", signal),
                ],
            );
            daemon_log::log_stderr_since(&log_file, stderr_offset);
            daemon_log::capture_syspolicy("child_exit");
            return SpawnResult::Failed;
        }
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if now >= next_report {
            let secs = started.elapsed().as_secs().to_string();
            lifecycle_log(&format!(
                "try_spawn_daemon: pid {} still initializing (system policy evaluation), waiting...",
                pid
            ));
            daemon_log::log(
                "still_initializing",
                &[("pid", pid.to_string()), ("elapsed_s", secs)],
            );
            next_report = now + Duration::from_secs(10);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if is_process_alive(pid) {
        lifecycle_log(&format!(
            "try_spawn_daemon: pid {} NOT responding after ~30s but STILL ALIVE; keep it, watchdog will verify",
            pid
        ));
        daemon_log::log(
            "timeout",
            &[
                ("pid", pid.to_string()),
                ("elapsed_s", "30".to_string()),
                ("alive", "true".to_string()),
                ("decision", "keep_pending".to_string()),
            ],
        );
        daemon_log::log_stderr_since(&log_file, stderr_offset);
        daemon_log::capture_syspolicy("timeout_alive");
        PENDING_DAEMON_PID.store(pid, Ordering::SeqCst);
        std::mem::forget(child);
        SpawnResult::Pending(pid)
    } else {
        lifecycle_log(&format!(
            "try_spawn_daemon: pid {} gone after ~30s without listening; stderr in daemon log",
            pid
        ));
        daemon_log::log(
            "timeout",
            &[
                ("pid", pid.to_string()),
                ("elapsed_s", "30".to_string()),
                ("alive", "false".to_string()),
                ("decision", "fail".to_string()),
            ],
        );
        daemon_log::log_stderr_since(&log_file, stderr_offset);
        daemon_log::capture_syspolicy("timeout_gone");
        let _ = child.kill();
        let _ = child.wait();
        SpawnResult::Failed
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

/// 守护自愈看门狗：App 生命周期内每 5s 探一次 9876，连续两次失败且距上次
/// spawn 超过 12s 时重新拉起守护。
///
/// 为什么需要：spawn_sidecar 此前只在 setup 跑一次，一旦某次启动被系统策略
/// 评估拖慢或被 AMFI 拦下，整个 App 会话里守护永远不会再起来，面板「原生守护」
/// 一直灰，用户只能手动重启 App。看门狗把「一次性启动」变成「持续保证」。
pub fn start_sidecar_watchdog() {
    std::thread::spawn(|| {
        let mut failures = 0u32;
        // 指数退避：5s → 10s → 20s → 40s（上限）。避免慢启动机器上
        // 「spawn → 系统评估超时 → kill → 再 spawn」形成高频自伤循环。
        let mut backoff_secs = 5u64;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
            if WATCHDOG_STOP.load(Ordering::SeqCst) {
                lifecycle_log("watchdog: stop requested, exiting");
                break;
            }
            // 用 ping 响应判定健康，而非裸 TCP connect：
            // 守护「LISTEN 但假死 / 蓝牙冷启动占住事件循环」时 connect 会成功，
            // 旧逻辑据此 failures=0，永远不自愈——这正是面板灰但守护进程还在的表现。
            if daemon_reachable(400, 1500) {
                if failures > 0 || backoff_secs > 5 {
                    lifecycle_log("watchdog: daemon ping OK, reset failures/backoff");
                    daemon_log::log("watchdog_recovered", &[("failures", failures.to_string())]);
                }
                failures = 0;
                backoff_secs = 5;
                continue;
            }
            failures += 1;
            daemon_log::log(
                "watchdog_fail",
                &[
                    ("failures", failures.to_string()),
                    ("backoff_s", backoff_secs.to_string()),
                    ("port_listening", port_listening().to_string()),
                ],
            );
            if failures < 2 {
                continue;
            }
            // 上一轮 spawn 可能正在等 macOS 27 的策略评估（最长 30s）。
            // 此时排队只会重复拉起守护，必须在拿锁前就跳过。
            if SPAWN_IN_PROGRESS.load(Ordering::SeqCst) {
                daemon_log::log("watchdog_skip", &[("reason", "spawn_in_progress".to_string())]);
                continue;
            }
            let last = LAST_SPAWN_DONE.load(Ordering::SeqCst);
            // 0 = setup 首轮 spawn 还在进行；别插队。
            // 有 Pending 守护时给足 90s 宽限：它只是慢，不是死，别再 kill 它。
            let pending = PENDING_DAEMON_PID.load(Ordering::SeqCst) != 0;
            let grace = if pending { 90 } else { 12 };
            let since = unix_secs().saturating_sub(last);
            if last == 0 || since < grace {
                daemon_log::log(
                    "watchdog_skip",
                    &[
                        ("reason", "cooldown".to_string()),
                        ("since_last_s", since.to_string()),
                        ("grace_s", grace.to_string()),
                        ("pending_pid", PENDING_DAEMON_PID.load(Ordering::SeqCst).to_string()),
                    ],
                );
                continue;
            }
            // 二次验证：拿锁前守护可能刚好恢复（首轮 spawn 慢启动完成）。
            // 不复核就 spawn 会先 kill_port_holder 杀掉健康守护 → 亮→灰→亮反复。
            if daemon_reachable(400, 1500) {
                lifecycle_log("watchdog: daemon recovered before respawn, skip");
                daemon_log::log("watchdog_skip", &[("reason", "recovered_on_recheck".to_string())]);
                failures = 0;
                backoff_secs = 5;
                continue;
            }
            lifecycle_log(&format!(
                "watchdog: daemon unreachable twice, respawning (backoff={}s)",
                backoff_secs
            ));
            daemon_log::log(
                "watchdog_respawn",
                &[
                    ("failures", failures.to_string()),
                    ("backoff_s", backoff_secs.to_string()),
                    ("pending_pid", PENDING_DAEMON_PID.load(Ordering::SeqCst).to_string()),
                ],
            );
            spawn_sidecar("watchdog"); // 内部持 SPAWN_LOCK，与 setup 的 spawn 串行
            failures = 0;
            backoff_secs = (backoff_secs * 2).min(40);
        }
    });
}

/// 退出前调用：阻止看门狗在 shutdown_sidecar 清掉端口后又拉起守护。
pub fn stop_sidecar_watchdog() {
    WATCHDOG_STOP.store(true, Ordering::SeqCst);
}

fn main() {
    // 结构化守护诊断日志：一次会话一个 session id，任何线程可写。
    daemon_log::init_session();
    let mut builder = tauri::Builder::default()
        // 单实例：launchd 拉起或手动双开时，新进程立即退出并唤起已有面板，
        // 避免「开机自启后菜单栏出现两个图标」。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // deep-link 集成：登录回调经新进程传入时转给 auth 模块；
            // 同时 plugin 也会触发 on_open_url，重复 URL 由一次性通道自动忽略。
            for arg in &argv {
                // 兼容带/不带尾斜杠的回调形态，统一交给 auth 做严格判定
                if arg.starts_with("unoswitchkit://callback") {
                    crate::auth::handle_callback_url(arg.clone());
                }
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        // 登录回调：unoswitchkit://callback/?code=&state= 唤醒等待中的 auth_login
        .plugin(tauri_plugin_deep_link::init())
        // 全局快捷键：Rust 侧注册（不经前端 IPC），只在后台线程调用注册/注销，
        // 因为插件在 macOS 上会 dispatch 到主线程并同步等待。
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // accessory 模式：无 Dock 图标、无 App Switcher 条目
            let _ = app
                .handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);

            // OAuth 登录回调（unoswitchkit://callback/...）。
            use tauri_plugin_deep_link::DeepLinkExt;
            let _ = app.deep_link().on_open_url(|event| {
                for url in event.urls() {
                    crate::auth::handle_callback_url(url.to_string());
                }
            });

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

            // 拉起守护进程（独立常驻，不持有句柄）。放到后台线程：就绪窗口
            // 最坏 9s，不能让它阻塞面板出现；看门狗在 App 生命周期内持续兜底。
            start_sidecar_watchdog();
            std::thread::spawn(|| spawn_sidecar("setup"));

            // 登录令牌续期：后台线程，启动后每 5 分钟静默刷新过期会话。
            // is_logged_in 保持纯本地判定（零延迟），不在此处阻塞命令线程。
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(300));
                    crate::auth::refresh_if_needed(&handle);
                });
            }

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
                sysmon: Mutex::new(crate::sysmon::SystemMonitor::new()),
            });

            // 恢复持久化的全局快捷键。必须在后台线程：插件在 macOS 上把注册
            // dispatch 到主线程并阻塞等待，主线程里调用会死锁；稍等 runloop 起来。
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    crate::shortcuts::restore_on_startup(&handle);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_toggle_states,
            commands::get_system_stats,
            commands::get_appearance_mode,
            commands::set_appearance_mode,
            commands::cycle_appearance_mode,
            commands::open_system_screenshot,
            commands::set_shortcut_bindings,
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
            auth::auth_status,
            auth::auth_login,
            auth::auth_login_cancel,
            auth::auth_logout,
            permissions::get_permission_state,
            permissions::refresh_permissions,
            permissions::acknowledge_permission_change,
            permissions::copy_reset_command,
            permissions::open_privacy_settings,
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
