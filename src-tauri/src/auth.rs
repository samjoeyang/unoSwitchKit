//! UNOAuth OAuth 2.1 / OIDC 登录与功能门控（唯一权威，Rust 侧）。
//!
//! 桌面场景按 RFC 8252 走 Authorization Code + PKCE + 自定义 URL scheme 回调：
//!   1. `auth_login` 生成 PKCE verifier + state，注册一次性回调通道；
//!   2. 系统浏览器打开 UNOAuth 授权页（client_id + challenge + state）；
//!   3. 用户在浏览器登录，重定向到 `unoswitchkit://callback/?code=&state=`；
//!   4. deep-link 插件收到 URL → `handle_callback_url` 唤醒等待中的登录；
//!   5. POST token 端点（public client，body 带 client_id + PKCE）换 token；
//!   6. 会话落盘 app_config_dir/session.json（权限 600）；
//!   7. 所有登录层命令在 Rust 侧调用 `is_logged_in()` 门控，前端判断不可信。
//!
//! client 为 public（服务端仅登记 client_id + redirect_uri，无 secret）；
//! 已实测 token 端点接受裸 client_id + PKCE，带 Basic 头反而报 invalid_client。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const AUTHORIZE_URL: &str = "https://auth.zhenzhidaole.com/api/auth/oauth2/authorize";
const TOKEN_URL: &str = "https://auth.zhenzhidaole.com/api/auth/oauth2/token";
const USERINFO_URL: &str = "https://auth.zhenzhidaole.com/api/auth/oauth2/userinfo";
const REVOKE_URL: &str = "https://auth.zhenzhidaole.com/api/auth/oauth2/revoke";

/// 服务端登记的 public client（实测 authorize 接受该 id + 回调地址）。
const CLIENT_ID: &str = "GCiqJXOkplapPOBUnUJcDBKCjzjITHlJ";
const REDIRECT_URI: &str = "unoswitchkit://callback/";

/// 登录是否进行中（防重复点）；等待回调的通道由 deep-link 回调唤醒。
static LOGIN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static CALLBACK_TX: Mutex<Option<mpsc::Sender<String>>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Clone, Default)]
struct Session {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    sub: String,
    email: String,
    name: String,
}

/// 前端可见状态（绝不含 token）。
#[derive(Serialize, Clone)]
pub struct AuthStatus {
    pub logged_in: bool,
    pub configured: bool,
    pub email: String,
    pub name: String,
    pub provider: String,
}

/// 认证专用日志：登录链路排查用，独立于守护/生命周期日志。
/// 只记录结构与 state 比对结果，token/code 一律脱敏。
pub fn auth_log(msg: &str) {
    use std::io::Write;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let path = PathBuf::from(home)
        .join("Library")
        .join("Logs")
        .join("switchkit-auth.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{}] {}", now_secs(), msg);
    }
}

fn session_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("session.json"))
}

fn load_session(app: &AppHandle) -> Option<Session> {
    let p = session_path(app)?;
    let text = fs::read_to_string(&p).ok()?;
    serde_json::from_str::<Session>(&text).ok()
}

fn save_session(app: &AppHandle, s: &Session) -> Result<(), String> {
    let p = session_path(app).ok_or("无法定位配置目录")?;
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn clear_session(app: &AppHandle) {
    if let Some(p) = session_path(app) {
        let _ = fs::remove_file(&p);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn session_valid(s: &Session) -> bool {
    if s.access_token.is_empty() {
        return false;
    }
    s.expires_at == 0 || s.expires_at > now_secs() + 60
}

/// 功能门控唯一入口。
pub fn is_logged_in(app: &AppHandle) -> bool {
    load_session(app).map(|s| session_valid(&s)).unwrap_or(false)
}

pub fn require_login() -> String {
    "该功能需要登录 SwitchKit 账号，请在「设置」中登录".to_string()
}

// ---------------- deep-link 回调 ----------------

/// 由 main.rs 的 deep-link 监听器调用。把回调 URL 交给等待中的 auth_login。
///
/// 兼容两种回调形态（服务端可能把 redirect_uri 规范化掉尾斜杠）：
///   unoswitchkit://callback/?code=...&state=...
///   unoswitchkit://callback?code=...&state=...
/// 只接受 callback 路径，防止任意 scheme URL 误唤醒登录。
pub fn handle_callback_url(url: String) {
    let prefix = "unoswitchkit://callback";
    let rest = url.strip_prefix(prefix).unwrap_or("");
    // 只接受 callback 后接 / ? # 或结尾；callback-evil 之类不算
    if !(rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#')) {
        auth_log("callback: 忽略非回调 URL");
        return;
    }
    // 只记结构，不记 code/token
    let shape = match url.split_once('?') {
        Some((base, q)) => format!(
            "{}?keys=[{}]",
            base,
            q.split('&')
                .filter_map(|p| p.split_once('=').map(|(k, _)| k))
                .collect::<Vec<_>>()
                .join(",")
        ),
        None => url.clone(),
    };
    auth_log(&format!("callback: 收到 {} (len={})", shape, url.len()));
    if let Ok(guard) = CALLBACK_TX.lock() {
        if let Some(tx) = guard.as_ref() {
            match tx.send(url) {
                Ok(_) => auth_log("callback: 已送入手等待通道"),
                Err(_) => auth_log("callback: 通道已关闭（可能超时/重复回调）"),
            }
        } else {
            auth_log("callback: 无等待中的登录（未点登录或已超时）");
        }
    }
}

// ---------------- PKCE / 编码工具 ----------------

fn random_b64url() -> Result<String, String> {
    let mut buf = [0u8; 32];
    use std::io::Read;
    let mut f = fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(base64url(&buf))
}

fn base64url(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((data.len() * 4 + 2) / 3);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
    }
    out
}

/// code_challenge = BASE64URL(SHA256(verifier))，用系统 shasum（绝对路径）。
fn pkce_challenge(verifier: &str) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法计算 PKCE challenge: {}", e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(verifier.as_bytes()).ok();
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let hex = text.split_whitespace().next().unwrap_or("");
    if hex.len() != 64 {
        return Err("PKCE 哈希输出异常".to_string());
    }
    let bytes: Vec<u8> = (0..32)
        .filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect();
    if bytes.len() != 32 {
        return Err("PKCE 哈希解码失败".to_string());
    }
    Ok(base64url(&bytes))
}

// ---------------- HTTP（系统 curl，复用 balance.rs 模式） ----------------

/// POST x-www-form-urlencoded（public client：不传 Basic 头，client_id 放 body）。
fn http_post_form(url: &str, body: &str) -> Result<(u16, String), String> {
    let mut cmd = Command::new("/usr/bin/curl");
    cmd.args(["-sS", "-m", "20", "-X", "POST"]);
    cmd.args(["-H", "Content-Type: application/x-www-form-urlencoded"]);
    cmd.args(["--data", body]);
    cmd.args(["-w", "\\n%{http_code}", url]);
    let out = cmd.output().map_err(|e| format!("无法执行 curl: {}", e))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = match text.rfind('\n') {
        Some(i) => (
            text[..i].to_string(),
            text[i + 1..].trim().parse::<u16>().unwrap_or(0),
        ),
        None => (text.clone(), 0),
    };
    Ok((code, body))
}

fn http_get_bearer(url: &str, token: &str) -> Result<serde_json::Value, String> {
    let out = Command::new("/usr/bin/curl")
        .args([
            "-sS",
            "-m",
            "15",
            "-H",
            &format!("Authorization: Bearer {}", token),
            url,
        ])
        .output()
        .map_err(|e| format!("无法执行 curl: {}", e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {}", e))
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 百分号解码（application/x-www-form-urlencoded：+ 视作空格）。
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = |b: u8| -> Option<u8> {
                    match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        b'A'..=b'F' => Some(b - b'A' + 10),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (h(bytes[i + 1]), h(bytes[i + 2])) {
                    out.push((a << 4) | b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 从回调 URL 解析 code & state（值做百分号解码）。
fn parse_callback(url: &str) -> Option<(String, String)> {
    let query = url.split_once('?').map(|(_, q)| q).or_else(|| url.split_once('#').map(|(_, q)| q))?;
    let (mut code, mut state) = (String::new(), String::new());
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        let k = kv.next().unwrap_or("");
        let v = urldecode(kv.next().unwrap_or(""));
        if k == "code" {
            code = v;
        } else if k == "state" {
            state = v;
        }
    }
    if code.is_empty() {
        None
    } else {
        Some((code, state))
    }
}

// ---------------- 命令 ----------------

#[tauri::command]
pub fn auth_status(app: AppHandle) -> AuthStatus {
    let s = load_session(&app).unwrap_or_default();
    let logged = session_valid(&s);
    AuthStatus {
        logged_in: logged,
        configured: true,
        email: if logged { s.email } else { String::new() },
        name: if logged { s.name } else { String::new() },
        provider: if logged { "UNOAuth".to_string() } else { String::new() },
    }
}

/// 登录结果事件名（前端 listen 此事件更新 UI）。
pub const LOGIN_EVENT: &str = "auth://login-result";

#[derive(Serialize, Clone)]
pub struct LoginEvent {
    pub ok: bool,
    pub message: String,
}

/// 非阻塞登录：生成 PKCE + 打开浏览器后立即返回。
/// 真正的 token 交换在后台线程完成，通过 emit(LOGIN_EVENT) 通知前端。
/// 这样前端 invoke 不会 pending，Webview 转圈不会卡死；用户在浏览器里
/// 花多久（含注册、找密码、二次验证）都不受命令超时限制。
#[tauri::command]
pub fn auth_login(app: AppHandle) -> Result<(), String> {
    if !LOGIN_IN_FLIGHT.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        return Err("登录已在进行中".to_string());
    }
    let verifier = random_b64url()?;
    let challenge = pkce_challenge(&verifier)?;
    let state = random_b64url()?;

    let (tx, rx) = mpsc::channel::<String>();
    if let Ok(mut guard) = CALLBACK_TX.lock() {
        *guard = Some(tx);
    }

    let authorize = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        AUTHORIZE_URL,
        urlencode(CLIENT_ID),
        urlencode(REDIRECT_URI),
        urlencode("openid profile email offline_access"),
        urlencode(&state),
        urlencode(&challenge),
    );
    auth_log(&format!(
        "login: authorize URL 已生成 (redirect={}, scope=openid profile email offline_access)",
        REDIRECT_URI
    ));
    // 绝对路径 /usr/bin/open：GUI 进程 PATH 仅 /usr/bin:/bin
    if let Err(e) = Command::new("/usr/bin/open").arg(&authorize).output() {
        let _ = CALLBACK_TX.lock().map(|mut g| g.take());
        LOGIN_IN_FLIGHT.store(false, Ordering::SeqCst);
        auth_log(&format!("login: 打开浏览器异常: {}", e));
        return Err(format!("打开浏览器失败: {}", e));
    }
    auth_log("login: 浏览器已唤起，后台等待回调（600s）");

    // 后台线程等待回调并完成交换，全程不占 Webview 的命令通道
    let app_bg = app.clone();
    std::thread::spawn(move || {
        use tauri::Emitter;
        let outcome = login_wait_and_finish(&app_bg, rx, verifier, state);
        let _ = CALLBACK_TX.lock().map(|mut g| g.take());
        LOGIN_IN_FLIGHT.store(false, Ordering::SeqCst);
        let event = match outcome {
            Ok(s) => LoginEvent { ok: true, message: format!("登录成功：{}", s.email) },
            Err(e) => LoginEvent { ok: false, message: e },
        };
        auth_log(&format!("login: emit result ok={} msg={}", event.ok, event.message));
        let _ = app_bg.emit(LOGIN_EVENT, event);
    });

    Ok(())
}

/// 取消进行中的登录（关闭通道，后台线程会收到断开错误并 emit 失败事件）。
#[tauri::command]
pub fn auth_login_cancel() -> Result<(), String> {
    let had = CALLBACK_TX.lock().map(|mut g| g.take()).unwrap_or(None).is_some();
    auth_log(&format!("login: 取消请求 had_pending={}", had));
    Ok(())
}

fn login_wait_and_finish(app: &AppHandle, rx: mpsc::Receiver<String>, verifier: String, state: String) -> Result<AuthStatus, String> {
    let wait_result = rx.recv_timeout(Duration::from_secs(600));
    let url = wait_result.map_err(|_| {
        auth_log("login: 等待回调超时 600s");
        "登录超时（10 分钟）。若已完成网页登录，请再点一次登录重试。".to_string()
    })?;
    auth_log("login: 收到回调，开始解析");
    let (code, got_state) = parse_callback(&url).ok_or_else(|| {
        auth_log("login: 回调缺少 code 参数");
        "登录回调缺少授权码".to_string()
    })?;
    auth_log(&format!(
        "login: code len={}, state 匹配={}",
        code.len(),
        got_state == state
    ));
    if got_state != state {
        auth_log("login: state 不匹配，放弃");
        return Err("登录校验失败（state 不匹配），请重试".to_string());
    }

    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencode(&code),
        urlencode(REDIRECT_URI),
        urlencode(CLIENT_ID),
        urlencode(&verifier),
    );
    let (status, body) = http_post_form(TOKEN_URL, &form)?;
    auth_log(&format!("login: token 端点 HTTP {}", status));
    if status < 200 || status >= 300 {
        auth_log(&format!("login: token 失败响应={}", body.chars().take(200).collect::<String>()));
        return Err(format!(
            "换取登录凭证失败（HTTP {}）：{}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }
    let token: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("解析登录凭证失败: {}", e))?;
    let access = token
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if access.is_empty() {
        return Err("登录响应缺少 access_token".to_string());
    }
    let refresh = token
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let expires_in = token.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);

    let (mut sub, mut email, mut name) = (String::new(), String::new(), String::new());
    if let Ok(info) = http_get_bearer(USERINFO_URL, &access) {
        sub = info
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        email = info
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        name = info
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| info.get("preferred_username").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
    }
    let session = Session {
        access_token: access,
        refresh_token: refresh,
        expires_at: now_secs() + expires_in,
        sub,
        name: if name.is_empty() { email.clone() } else { name },
        email,
    };
    save_session(app, &session)?;
    auth_log("login: 会话已保存，登录完成");

    Ok(AuthStatus {
        logged_in: true,
        configured: true,
        email: session.email,
        name: session.name,
        provider: "UNOAuth".to_string(),
    })
}

#[tauri::command]
pub fn auth_logout(app: AppHandle) -> Result<(), String> {
    if let Some(s) = load_session(&app) {
        if !s.refresh_token.is_empty() {
            let form = format!(
                "token={}&token_type_hint=refresh_token&client_id={}",
                urlencode(&s.refresh_token),
                urlencode(CLIENT_ID),
            );
            let _ = http_post_form(REVOKE_URL, &form);
        }
    }
    clear_session(&app);
    Ok(())
}

/// token 即将过期（<10 分钟）时用 refresh_token 静默续期；失败清除会话。
pub fn refresh_if_needed(app: &AppHandle) -> bool {
    let Some(s) = load_session(app) else { return false };
    if s.expires_at > now_secs() + 600 {
        return true;
    }
    if s.refresh_token.is_empty() {
        return false;
    }
    let form = format!(
        "grant_type=refresh_token&refresh_token={}&client_id={}",
        urlencode(&s.refresh_token),
        urlencode(CLIENT_ID),
    );
    let Ok((status, body)) = http_post_form(TOKEN_URL, &form) else {
        clear_session(app);
        return false;
    };
    if status < 200 || status >= 300 {
        clear_session(app);
        return false;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        clear_session(app);
        return false;
    };
    let access = v
        .get("access_token")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if access.is_empty() {
        clear_session(app);
        return false;
    }
    let next = Session {
        access_token: access,
        refresh_token: v
            .get("refresh_token")
            .and_then(|x| x.as_str())
            .unwrap_or(&s.refresh_token)
            .to_string(),
        expires_at: now_secs() + v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(3600),
        sub: s.sub,
        email: s.email,
        name: s.name,
    };
    save_session(app, &next).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_known_values() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff, 0x00]), "-_8A");
    }

    #[test]
    fn urlencode_basic() {
        assert_eq!(urlencode("a b+c&d"), "a%20b%2Bc%26d");
    }

    #[test]
    fn pkce_challenge_matches_rfc_vector() {
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_challenge(v).unwrap(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn callback_parsing() {
        let url = "unoswitchkit://callback/?code=abc&state=xyz";
        assert_eq!(parse_callback(url), Some(("abc".to_string(), "xyz".to_string())));
        assert_eq!(parse_callback("unoswitchkit://callback/?state=xyz"), None);
    }

    #[test]
    fn callback_parsing_no_slash_and_encoded() {
        let url = "unoswitchkit://callback?code=a%2Bb%3D%3D&state=x%2Dy";
        assert_eq!(
            parse_callback(url),
            Some(("a+b==".to_string(), "x-y".to_string()))
        );
    }

    #[test]
    fn callback_prefix_accepts_both_shapes() {
        // 校验放宽后的前缀：带斜杠与不带斜杠的回调都应被接受
        for u in [
            "unoswitchkit://callback/?code=a&state=b",
            "unoswitchkit://callback?code=a&state=b",
        ] {
            assert!(u.starts_with("unoswitchkit://callback"), "{}", u);
        }
        let rest = "unoswitchkit://callback-evil?code=a"
            .strip_prefix("unoswitchkit://callback")
            .unwrap_or("");
        assert!(!(rest.is_empty()
            || rest.starts_with('/')
            || rest.starts_with('?')
            || rest.starts_with('#')));
    }
}
