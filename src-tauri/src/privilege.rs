use crate::util;

/// MVP 特权提升通道：通过 osascript 弹出系统密码框执行 root 命令。
///
/// 设计目标（NFR-4）：
/// - 凭据不落盘、不进日志、不硬编码。
/// - 生产环境应替换为 SMJobBless privileged helper（见设计文档 §4.5）。
///
/// 传入的 `script` 会经由 /bin/sh 执行（osascript 的 do shell script 即 sh），
/// 因此可包含 `;` 串联的多个命令。
pub fn run_as_root(script: &str) -> Result<(), String> {
    // 转义 AppleScript 字符串字面量中的特殊字符
    let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
    let apple = format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    );
    match util::run_osascript(&apple) {
        Ok(_) => Ok(()),
        Err(e) => {
            // 用户取消授权时 osascript 报错含 "User canceled"
            if e.contains("User canceled") || e.contains("-128") {
                Err("已取消授权".to_string())
            } else {
                Err(e)
            }
        }
    }
}
