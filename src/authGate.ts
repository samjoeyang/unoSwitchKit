import { getAuth } from "./authStore";

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
