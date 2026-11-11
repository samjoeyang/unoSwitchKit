//! 全局快捷键：动作白名单 + 持久化绑定 + 注册结果逐项反馈。
//!
//! 边界（与功能评估结论一致）：
//! - 只注册「至少一个修饰键 + 一个普通键」的组合键。tauri-apps/global-hotkey 在
//!   macOS 上用 Carbon RegisterEventHotKey，普通组合键不需要辅助功能 / 输入监控权限；
//!   单键、媒体键会落到 CGEventTap，需要 system-wide TCC，本项目不做。
//! - 快捷键注册/注销在 macOS 上会被插件 dispatch 到主线程并同步等待，
//!   因此 `apply_bindings` 只能在后台线程 / blocking 线程池调用，绝不能在主线程调用。
//! - 动作只复用已有的零新增权限命令；采集类（截屏/录音/录屏）不在此列。

use std::collections::HashMap;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::commands;
use crate::models::ShortcutReport;

/// 允许绑定的动作 id。前端用同一份顺序渲染中文标签。
pub const ACTION_IDS: &[&str] = &[
    "toggle_panel",
    "cycle_appearance",
    "open_screenshot",
    "lock_screen",
    "clear_clipboard",
    "screensaver",
    "empty_trash",
    "hide_desktop",
    "hide_dock",
];

pub fn is_valid_action(action: &str) -> bool {
    ACTION_IDS.contains(&action)
}

/// 允许作为主键的普通键（与前端 keyFromCode 白名单一致）。
/// 媒体键 / 亮度键 / 电源键不在其中：global-hotkey 在 macOS 上会用 CGEventTap
/// 处理它们，落入输入监控类 TCC，超出本轮「零新增权限」范围。
const ALLOWED_MAIN_KEYS: &[&str] = &[
    "SPACE", "ENTER", "TAB", "ESCAPE", "BACKSPACE", "DELETE", "HOME", "END",
    "PAGEUP", "PAGEDOWN", "ARROWUP", "ARROWDOWN", "ARROWLEFT", "ARROWRIGHT",
    "MINUS", "EQUAL", "BRACKETLEFT", "BRACKETRIGHT", "SEMICOLON", "QUOTE",
    "COMMA", "PERIOD", "SLASH", "BACKSLASH", "BACKQUOTE",
];

fn is_supported_main_key(key: &str) -> bool {
    let u = key.to_uppercase();
    let b = u.as_bytes();
    if b.len() == 4 && u.starts_with("KEY") && b[3].is_ascii_uppercase() {
        return true;
    }
    if b.len() == 6 && u.starts_with("DIGIT") && b[5].is_ascii_digit() {
        return true;
    }
    if let Some(n) = u.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=12).contains(&n) {
            return true;
        }
    }
    ALLOWED_MAIN_KEYS.contains(&u.as_str())
}

/// 组合键必须带修饰键，且主键在普通键白名单内（拒绝单键、媒体键、纯修饰键）。
fn is_valid_combo(combo: &str) -> bool {
    let parts: Vec<&str> = combo.split('+').map(|p| p.trim()).collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    let mods = [
        "COMMAND", "CMD", "SUPER", "META", "CONTROL", "CTRL", "OPTION", "ALT", "SHIFT",
        "COMMANDORCONTROL", "COMMANDORCTRL", "CMDORCTRL", "CMDORCONTROL",
    ];
    if parts[..parts.len() - 1]
        .iter()
        .any(|p| !mods.contains(&p.to_uppercase().as_str()))
    {
        return false;
    }
    is_supported_main_key(parts.last().unwrap())
}

/// 执行一个白名单动作。面板按钮与快捷键回调共用同一入口。
pub async fn execute_action(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        "toggle_panel" => {
            if let Some(w) = app.get_webview_window("main") {
                if w.is_visible().unwrap_or(false) {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            Ok(())
        }
        "cycle_appearance" => commands::cycle_appearance_mode().await.map(|_| ()),
        "open_screenshot" => commands::open_system_screenshot().await,
        "lock_screen" => commands::lock_screen().await,
        "clear_clipboard" => commands::clear_clipboard().await,
        "screensaver" => commands::start_screensaver().await,
        "empty_trash" => commands::empty_trash().await.map(|_| ()),
        "hide_desktop" => commands::toggle_hide_desktop().await.map(|_| ()),
        "hide_dock" => commands::toggle_hide_dock().await.map(|_| ()),
        _ => Err(format!("未知快捷键动作：{}", action)),
    }
}

/// 注销全部旧绑定并重新注册。返回逐项结果，供设置页提示冲突。
///
/// 注意：先 `unregister_all()` 再逐条 `on_shortcut()`；若某条失败（组合键被系统或
/// 其它 App 占用），其它绑定不受影响。配置已经由调用方落盘，下次启动会自动重试。
pub fn apply_bindings(app: &AppHandle, bindings: &HashMap<String, String>) -> Vec<ShortcutReport> {
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister_all() {
        eprintln!("[shortcuts] unregister_all 失败：{}", e);
    }

    let mut reports = Vec::new();
    let mut used: HashMap<String, String> = HashMap::new();

    // 配置里出现未知动作时显式反馈，不静默丢弃（防止旧版本/手改配置留下死绑定）。
    for (action, combo) in bindings {
        if !is_valid_action(action) {
            reports.push(ShortcutReport {
                action: action.clone(),
                shortcut: combo.clone(),
                ok: false,
                error: Some("未知动作，已忽略".into()),
            });
        }
    }

    // 固定顺序输出，避免 HashMap 迭代顺序让设置页反馈乱跳。
    for action in ACTION_IDS {
        let Some(combo) = bindings.get(*action) else { continue };
        let combo = combo.trim();
        if combo.is_empty() {
            continue;
        }
        let base = |ok: bool, error: Option<String>| ShortcutReport {
            action: (*action).to_string(),
            shortcut: combo.to_string(),
            ok,
            error,
        };

        if !is_valid_combo(combo) {
            reports.push(base(false, Some("需至少一个修饰键 + 一个普通按键".into())));
            continue;
        }
        if let Some(other) = used.get(combo) {
            reports.push(base(false, Some(format!("与「{}」使用了相同组合键", other))));
            continue;
        }

        let action_owned = (*action).to_string();
        let handle = app.clone();
        let result = gs.on_shortcut(combo, move |_app, _shortcut, event| {
            if matches!(event.state, ShortcutState::Pressed) {
                let h = handle.clone();
                let a = action_owned.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = execute_action(h, a).await;
                });
            }
        });

        match result {
            Ok(()) => {
                used.insert(combo.to_string(), (*action).to_string());
                reports.push(base(true, None));
            }
            Err(e) => reports.push(base(
                false,
                Some(format!("注册失败（可能被系统或其它应用占用）：{}", e)),
            )),
        }
    }
    reports
}

/// 启动时从持久化设置恢复快捷键。调用方必须放在后台线程。
pub fn restore_on_startup(app: &AppHandle) {
    let settings = crate::store::load_settings(app);
    if settings.shortcut_bindings.is_empty() {
        return;
    }
    let reports = apply_bindings(app, &settings.shortcut_bindings);
    for r in reports.iter().filter(|r| !r.ok) {
        eprintln!("[shortcuts] 启动恢复 {} ({}) 失败：{:?}", r.action, r.shortcut, r.error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn frontend_generated_combos_parse() {
        for s in [
            "Command+Shift+KeyS",
            "CommandOrControl+Alt+Digit1",
            "Control+Shift+Space",
            "Command+F5",
        ] {
            assert!(Shortcut::from_str(s).is_ok(), "无法解析快捷键：{}", s);
        }
    }

    #[test]
    fn single_keys_and_media_keys_are_rejected() {
        assert!(!is_valid_combo("KeyS"));
        assert!(!is_valid_combo("MediaPlay"));
        assert!(!is_valid_combo("Command+MediaPlayPause"));
        assert!(!is_valid_combo("Command+AudioVolumeUp"));
        assert!(!is_valid_combo("Command+"));
        assert!(!is_valid_combo("Shift+Alt+"));
        assert!(!is_valid_combo("KeyS+Command"));
        assert!(is_valid_combo("Command+Shift+KeyS"));
        assert!(is_valid_combo("Alt+Digit1"));
        assert!(is_valid_combo("Command+F12"));
    }

    #[test]
    fn action_whitelist_is_closed() {
        assert!(is_valid_action("toggle_panel"));
        assert!(!is_valid_action("record_screen"));
        assert!(!is_valid_action("run_shell"));
    }
}
