import { getAuth, loadAuth } from "./authStore";

/// 全局「需要登录」提示回调，由 App 注册（打开设置面板并滚动到登录区）。
let requireLoginHandler: (() => void) | null = null;

export function setRequireLoginHandler(fn: () => void) {
  requireLoginHandler = fn;
}

/// 前端快捷判断：未登录时弹出提示并返回 false；调用方据此提前 return。
/// 注意：这只是 UX，真正的权限校验在 Rust 命令层（auth::require_login）。
export function gateFrontend(): boolean {
  if (getAuth().logged_in) return true;
  requireLoginHandler?.();
  return false;
}

/// 后端以「需要登录」拒绝命令时调用：前端登录态可能已过期（会话被续期流程清除、
/// 换包、手动改过配置等）。以 Rust 的 auth_status 为准重新同步，并给出准确文案。
export function authErrorText(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  if (!raw.includes("需要登录")) return raw;
  const wasLoggedIn = getAuth().logged_in;
  // 不 await：错误提示需要立即返回；同步在后台完成并刷新 UI
  void loadAuth();
  if (wasLoggedIn) requireLoginHandler?.();
  return wasLoggedIn
    ? "登录状态已失效，请在「设置 → 账号」重新登录"
    : "该功能需要登录 SwitchKit 账号";
}
