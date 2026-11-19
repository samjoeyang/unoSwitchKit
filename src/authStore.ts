import { useSyncExternalStore } from "react";
import { api } from "./ipc";
import type { AuthStatus } from "./types";

/// 全局登录态。App 启动时 load()，设置页登录/登出后 set()。
/// 各面板用 useAuth() 订阅，登录状态变化时自动重渲染，决定是否显示锁定态。
let current: AuthStatus = {
  logged_in: false,
  configured: false,
  email: "",
  name: "",
  provider: "",
};

const listeners = new Set<() => void>();

function emit() {
  listeners.forEach((l) => l());
}

export function setAuth(next: AuthStatus) {
  current = next;
  emit();
}

export async function loadAuth(): Promise<AuthStatus> {
  try {
    const s = await api.authStatus();
    setAuth(s);
    return s;
  } catch {
    setAuth({ logged_in: false, configured: false, email: "", name: "", provider: "" });
    return current;
  }
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getAuth(): AuthStatus {
  return current;
}

export function useAuth(): AuthStatus {
  return useSyncExternalStore(subscribe, getAuth, getAuth);
}
