use std::fs;
use std::path::PathBuf;

use tauri::Manager;

use crate::models::Settings;

fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.json"))
}

pub fn load_settings(app: &tauri::AppHandle) -> Settings {
    if let Some(p) = settings_path(app) {
        if let Ok(s) = fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<Settings>(&s) {
                return s;
            }
        }
    }
    Settings::default()
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
