use std::path::PathBuf;
use std::process::Command;

/// 解析用户主目录，命令与文件操作的根。
pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_string()))
}

/// 执行外部命令，返回 stdout；非零退出码视为错误（返回 stderr）。
pub fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("执行 '{}' 失败: {}", cmd, e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("'{}' 返回非零退出码 {:?}", cmd, output.status.code()))
        } else {
            Err(stderr.to_string())
        }
    }
}

/// 通过 osascript 执行 AppleScript。
pub fn run_osascript(script: &str) -> Result<String, String> {
    run("osascript", &["-e", script])
}
