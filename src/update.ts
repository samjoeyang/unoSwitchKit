import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface CheckResult {
  status: "latest" | "updated" | "error";
  message: string;
}

/// 检查并安装更新（仅在更新服务已配置时调用）。
/// 调用前请用 `api.getUpdateConfig()` 确认 enabled，未配置时插件未注册会报错。
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
