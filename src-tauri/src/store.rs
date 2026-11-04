use std::fs;
use std::path::PathBuf;

use tauri::Manager;

use crate::models::Settings;

fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.json"))
}

/// 当前 Settings 结构体的全部已知字段名（用于规范化时识别"未知遗留字段"）。
const KNOWN_SETTINGS_KEYS: &[&str] = &[
    "toggle_order",
    "cache_whitelist_extra",
    "last_release",
    "provider_keys",
    "http_proxy",
    "auto_clean_enabled",
    "auto_clean_interval_days",
    "last_auto_clean_ts",
];

/// 容错读取配置。
///
/// 关键修复：旧实现用 `serde_json::from_str::<Settings>()` 一次性整体反序列化，
/// 只要文件里任一字段（哪怕是已移除的旧字段 `menubar_*`）类型对不上新结构，
/// 整个解析就会失败并回退 `Settings::default()`——于是同一文件里的余额 apikey
/// 等所有设置被"连坐"清空。升级版本后 apikey 消失，正是这个机制导致的。
///
/// 现在改为逐字段解析：任一同级字段解析失败都只跳过该字段，绝不影响其它字段，
/// 余额 Key 因此永不因 schema 漂移而丢失。同时兼容旧字段名 `deepseek_keys`
/// （v0.3.8）→ 当前 `provider_keys`。
pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    let Some(p) = settings_path(app) else {
        return Settings::default();
    };
    let Ok(text) = fs::read_to_string(&p) else {
        return Settings::default();
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) else {
        eprintln!("[settings] settings.json 不是合法 JSON，使用默认配置");
        return Settings::default();
    };
    settings_from_value(&raw).unwrap_or_default()
}

/// 从已解析的 JSON 容错构建 Settings（纯函数，便于单测）。
/// 任一同级字段类型不符只跳过该字段，绝不连坐清空整个配置。
/// 余额 Key 兼容旧字段名 `deepseek_keys`（v0.3.8）与当前 `provider_keys`。
pub fn settings_from_value(raw: &serde_json::Value) -> Option<Settings> {
    let serde_json::Value::Object(obj) = raw else {
        return None;
    };
    let mut s = Settings::default();
    extract(&obj, "toggle_order", &mut s.toggle_order);
    extract(&obj, "cache_whitelist_extra", &mut s.cache_whitelist_extra);
    extract(&obj, "last_release", &mut s.last_release);
    s.provider_keys = extract_array::<crate::models::ProviderKey>(obj, "provider_keys")
        .or_else(|| extract_array::<crate::models::ProviderKey>(obj, "deepseek_keys"))
        .unwrap_or_default();
    extract(&obj, "http_proxy", &mut s.http_proxy);
    extract(&obj, "auto_clean_enabled", &mut s.auto_clean_enabled);
    extract(
        &obj,
        "auto_clean_interval_days",
        &mut s.auto_clean_interval_days,
    );
    extract(&obj, "last_auto_clean_ts", &mut s.last_auto_clean_ts);
    Some(s)
}

/// 把 JSON 对象中某个字段按目标类型取出；解析失败（类型不符/缺失）时原值不变。
fn extract<T: serde::de::DeserializeOwned>(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    out: &mut T,
) {
    if let Some(v) = obj.get(key) {
        if let Ok(x) = serde_json::from_value(v.clone()) {
            *out = x;
        }
    }
}

/// 把 JSON 对象中某个数组字段按目标元素类型取出；字段缺失或解析失败返回 None。
fn extract_array<T: serde::de::DeserializeOwned>(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<Vec<T>> {
    obj.get(key)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// 启动期调用一次：把读到的配置规范化写回磁盘。
///
/// 作用：① 把旧字段名 `deepseek_keys` 归一为 `provider_keys`，避免每次加载都靠
/// alias 兜底；② 剔除已删除功能遗留的未知字段（如 `menubar_*`），防止死数据长期
/// 残留；③ 仅当确有差异时才写盘，无谓 IO 最小化。文件无法解析时不覆盖，避免
/// 把损坏文件误写成空配置。
pub fn normalize_settings(app: &tauri::AppHandle) {
    let Some(p) = settings_path(app) else {
        return;
    };
    let Ok(text) = fs::read_to_string(&p) else {
        return;
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let serde_json::Value::Object(obj) = &raw else {
        return;
    };
    let has_legacy = obj.contains_key("deepseek_keys");
    let has_unknown = obj
        .keys()
        .any(|k| !KNOWN_SETTINGS_KEYS.contains(&k.as_str()));
    if !has_legacy && !has_unknown {
        return; // 已是最新 schema，无需写回
    }
    let s = load_settings(app);
    if let Err(e) = save_settings(app, &s) {
        eprintln!("[settings] 规范化配置写回失败：{e}");
    }
}

pub fn save_settings(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let p = settings_path(app).ok_or("无法定位配置目录")?;
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}

const LAUNCH_LABEL: &str = "com.switchkit.app";

fn launch_plist() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", LAUNCH_LABEL)),
    )
}

/// 注册 / 移除开机自启 LaunchAgent（FR-4）。
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    let plist = launch_plist().ok_or("无法定位 HOME")?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().to_string();
        let content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
  <key>Label</key><string>{label}</string>\n\
  <key>ProgramArguments</key>\n<array>\n<string>{exe}</string>\n</array>\n\
  <key>RunAtLoad</key><true/>\n\
  <key>ProcessType</key><string>Interactive</string>\n\
</dict>\n</plist>\n",
            label = LAUNCH_LABEL,
            exe = exe
        );
        if let Some(parent) = plist.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&plist, content).map_err(|e| e.to_string())?;
        // 幂等：若已加载过先卸载，避免重复注册
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap()])
            .output();
        std::process::Command::new("launchctl")
            .args(["load", plist.to_str().unwrap()])
            .output()
            .ok();
    } else if plist.exists() {
        std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap()])
            .output()
            .ok();
        fs::remove_file(&plist).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn is_launch_at_login() -> Result<bool, String> {
    Ok(launch_plist().map(|p| p.exists()).unwrap_or(false))
}

/// 启动时校验自启 plist 的可执行路径。
/// .app 被替换或移动后旧路径失效，launchd 仍指向旧位置会导致新版本不生效，
/// 甚至反复拉起已删除的副本。仅在正式构建中执行，避免 dev 二进制覆盖用户配置。
pub fn repair_launch_agent() {
    #[cfg(not(debug_assertions))]
    {
        let Some(plist) = launch_plist() else { return };
        if !plist.exists() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let exe_str = exe.to_string_lossy().to_string();
        let Ok(content) = fs::read_to_string(&plist) else { return };
        if content.contains(&format!("<string>{}</string>", exe_str)) {
            return;
        }
        let _ = set_launch_at_login(true);
    }
}

#[cfg(test)]
mod tests {
    use super::settings_from_value;
    use serde_json::json;

    /// 回归测试：升级后旧配置里混杂「已删除功能的未知字段（如 menubar_*）」时，
    /// 不能因整体反序列化失败而回退默认、把余额 apikey 一起清空。
    /// 这是「升级后 apikey 消失」bug 的直接复现与修复验证。
    #[test]
    fn sibling_unknown_field_does_not_wipe_keys() {
        let raw = json!({
            "toggle_order": [],
            "cache_whitelist_extra": [],
            "last_release": null,
            "menubar_managed": true,
            "menubar_positions": { "com.apple.controlcenter|电池": [1355.0, 1.0] },
            "provider_keys": [
                { "id": "1", "provider": "deepseek", "label": "主账号", "api_key": "sk-keep" }
            ]
        });
        let s = settings_from_value(&raw).expect("容错解析应成功");
        assert_eq!(s.provider_keys.len(), 1, "余额 Key 必须保留");
        assert_eq!(s.provider_keys[0].api_key, "sk-keep");
        assert_eq!(s.provider_keys[0].provider, "deepseek");
    }

    /// 回归测试：v0.3.8 的 `deepseek_keys` 与任意未知字段共存时，升级后 Key 应被
    /// 归一为 `provider_keys` 并保留。
    #[test]
    fn legacy_deepseek_keys_survive_with_unknown_fields() {
        let raw = json!({
            "menubar_managed": true,
            "deepseek_keys": [
                { "id": "1", "label": "主账号", "api_key": "sk-legacy" }
            ]
        });
        let s = settings_from_value(&raw).expect("容错解析应成功");
        assert_eq!(s.provider_keys.len(), 1, "旧 deepseek_keys 必须迁移");
        assert_eq!(s.provider_keys[0].api_key, "sk-legacy");
        assert_eq!(s.provider_keys[0].provider, ""); // 旧配置无 provider → 视为 deepseek
    }

    /// 异常防护：文件不是合法 JSON 时返回 None（由上层回退默认，不覆盖原文件）。
    #[test]
    fn invalid_json_returns_none() {
        let bad = serde_json::Value::String("not an object".into());
        assert!(settings_from_value(&bad).is_none());
    }

    /// 真实机制复现：某「已知字段」类型与当前结构不符（schema 漂移）时，
    /// 旧实现整体反序列化失败 → 回退默认 → 同文件里的余额 apikey 被清空；
    /// 新实现逐字段解析，坏字段被跳过并回退默认，apikey 仍保留。
    #[test]
    fn known_field_type_mismatch_keeps_keys() {
        let raw = json!({
            "last_release": "not-an-object", // 错误类型，应为 MemoryComparison 或 null
            "provider_keys": [
                { "id": "1", "provider": "deepseek", "label": "主账号", "api_key": "sk-keep" }
            ]
        });
        let s = settings_from_value(&raw).expect("容错解析应成功");
        assert_eq!(s.provider_keys.len(), 1, "余额 Key 必须保留");
        assert_eq!(s.provider_keys[0].api_key, "sk-keep");
        assert!(s.last_release.is_none(), "坏字段应被跳过并回退默认");
    }
}
