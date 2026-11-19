import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../ipc";
import { loadAuth, useAuth } from "../authStore";
import type { LoginEvent } from "../types";

/// 登录入口（放在「设置」面板内）。未登录显示登录按钮，已登录显示账号与登出。
/// 登录为非阻塞：点按钮只唤起浏览器，真正的交换在 Rust 后台线程完成后
/// 通过 auth://login-result 事件通知，避免 Webview invoke 长时间 pending 转圈。
export function AuthSection() {
  const auth = useAuth();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");

  // 监听后台登录结果（组件挂载期间有效）
  useEffect(() => {
    let alive = true;
    const unlisten = listen<LoginEvent>("auth://login-result", async (e) => {
      if (!alive) return;
      setBusy(false);
      if (e.payload.ok) {
        await loadAuth();
        setMsg(e.payload.message);
      } else {
        setMsg(`登录失败：${e.payload.message}`);
      }
    });
    return () => {
      alive = false;
      void unlisten.then((f) => f());
    };
  }, []);

  async function login() {
    setBusy(true);
    setMsg("");
    try {
      await api.authLogin();
      // 立即返回：仅表示浏览器已唤起，结果由事件通知
    } catch (e) {
      setBusy(false);
      setMsg(`启动登录失败：${e}`);
    }
  }

  async function cancel() {
    try {
      await api.authLoginCancel();
    } catch {
      // 取消失败不影响 UI：后台超时后仍会 emit 结果
    }
    setBusy(false);
    setMsg("已取消登录，可重新发起");
  }

  async function logout() {
    setBusy(true);
    setMsg("");
    try {
      await api.authLogout();
      await loadAuth();
      setMsg("已退出登录");
    } catch (e) {
      setMsg(`退出失败：${e}`);
    } finally {
      setBusy(false);
    }
  }

  const rowLabel = "text-[12px] text-white/85";
  const rowHint = "text-[10px] text-white/35";
  const actionBtn =
    "shrink-0 rounded bg-white/10 px-2.5 py-1 text-[10px] text-white/70 transition hover:bg-white/20 disabled:opacity-40";

  return (
    <div className="py-2" id="auth-section">
      <div className="flex items-center justify-between">
        <div>
          <div className={rowLabel}>账号</div>
          <div className={rowHint}>
            {!auth.configured
              ? "登录服务尚未配置"
              : auth.logged_in
                ? `${auth.email || auth.name || "已登录"} · ${auth.provider}`
                : "登录后解锁全部功能"}
          </div>
        </div>
        {auth.configured &&
          (auth.logged_in ? (
            <button onClick={() => void logout()} disabled={busy} className={actionBtn}>
              {busy ? "处理中…" : "退出登录"}
            </button>
          ) : busy ? (
            <button onClick={() => void cancel()} className={actionBtn}>
              取消登录
            </button>
          ) : (
            <button onClick={() => void login()} className={actionBtn}>
              登录
            </button>
          ))}
      </div>
      {busy && !auth.logged_in && (
        <div className="mt-1.5 text-[10px] leading-snug text-indigo-200/80">
          已打开系统浏览器，请在网页完成登录（10 分钟内）；完成后本面板自动解锁，无需停留在此窗口。
        </div>
      )}
      {msg && (
        <div className="mt-1 text-[10px] leading-snug text-white/45">{msg}</div>
      )}
      {!auth.configured && (
        <div className="mt-1 text-[10px] leading-snug text-amber-200/70">
          需在构建时注入 OAuth client 配置。
        </div>
      )}
    </div>
  );
}
