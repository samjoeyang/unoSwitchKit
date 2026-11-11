import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface CheckResult {
  status: "latest" | "updated" | "error";
  message: string;
}

/// 检查并安装更新（仅在更新服务已配置时调用）。
/// 调用前请用 `api.getUpdateConfig()` 确认 enabled，未配置时插件未注册会报错。
///
/// 换包后的「重新授权」提示由 Rust 侧签名哈希比对驱动
/// （permissions::get_permission_state）：ad-hoc 包每次 OTA 都会产生新 cdhash，
/// relaunch 后 App 重新挂载就会自动出现面板顶部的 PermissionBanner，
/// 因此这里不需要再写 localStorage / 埋点，避免两套状态互相打架。
export async function checkAndInstall(): Promise<CheckResult> {
  try {
    const update = await check();
    if (!update) {
      return { status: "latest", message: "已是最新版本" };
    }
    await update.downloadAndInstall();
    try {
      await relaunch();
    } catch {
      /* 极少数情况下 relaunch 需手动执行 */
    }
    return { status: "updated", message: `已更新到 v${update.version}` };
  } catch (e) {
    return { status: "error", message: `更新失败：${e}` };
  }
}
