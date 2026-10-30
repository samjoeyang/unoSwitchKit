import { useEffect, useState } from "react";
import { ToggleGrid, type ToggleKey } from "./components/ToggleGrid";
import { ResourcePanel } from "./components/ResourcePanel";
import { BluetoothSection } from "./components/BluetoothSection";
import { DeepSeekSection } from "./components/DeepSeekSection";
import { SettingsSection } from "./components/SettingsSection";
import { StatusDot } from "./components/StatusDot";
import { api } from "./ipc";
import { checkAndInstall } from "./update";
import type { ToggleStates } from "./types";

/// 应用版本（设置面板「检查更新」中展示）
const APP_VERSION = "0.3.9";

export default function App() {
  const [states, setStates] = useState<ToggleStates | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [launch, setLaunch] = useState(false);
  const [sidecar, setSidecar] = useState<boolean | null>(null);
  const [updateEnabled, setUpdateEnabled] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  // DeepSeek Key 变动版本号：设置页新增 / 余额区删除后递增，驱动双方同步
  const [dsVersion, setDsVersion] = useState(0);
  // 设置项操作提示（alert 在 WKWebView 中被禁用，改用面板内提示）
  const [notice, setNotice] = useState<string | null>(null);

  async function refresh() {
    try {
      setStates(await api.getToggleStates());
    } catch {
      /* 面板尚未就绪时忽略 */
    }
  }
  async function loadLaunch() {
    try {
      setLaunch(await api.isLaunchAtLogin());
    } catch {
      /* ignore */
    }
  }
  async function loadSidecar() {
    try {
      const s = await api.sidecarStatus();
      setSidecar(s.running);
    } catch {
      setSidecar(null);
    }
  }
  async function loadUpdateConfig() {
    try {
      const cfg = await api.getUpdateConfig();
      setUpdateEnabled(cfg.enabled);
    } catch {
      setUpdateEnabled(false);
    }
  }

  useEffect(() => {
    refresh();
    loadLaunch();
    loadSidecar();
    loadUpdateConfig();
    const t = setInterval(loadSidecar, 5000);
    // 面板每次重新显示时刷新（避免托盘弹出后显示过期状态，如废纸篓计数）
    const onVis = () => {
      if (!document.hidden) refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, []);

  // 设置面板展开在底栏上方，展开后滚到底部，保证面板与底栏都在可视区内
  useEffect(() => {
    if (!showSettings) return;
    const el = document.getElementById("root");
    if (!el) return;
    // 设置面板内的 Key 数量是异步读取的，布局会二次变化，故分两次贴底
    const stick = () => {
      el.scrollTop = el.scrollHeight;
    };
    const timers = [60, 260].map((ms) => window.setTimeout(stick, ms));
    return () => timers.forEach((t) => window.clearTimeout(t));
  }, [showSettings]);

  async function onToggle(key: ToggleKey) {
    setBusy(key);
    try {
      if (key === "dark_mode") await api.toggleDarkMode();
      else if (key === "desktop_hidden") await api.toggleHideDesktop();
      else if (key === "prevent_sleep") await api.togglePreventSleep();
      else if (key === "empty_trash") await api.emptyTrash();
      else if (key === "hide_dock") await api.toggleHideDock();
      else if (key === "show_hidden_files") await api.toggleShowHiddenFiles();
      else if (key === "screen_clean") await api.toggleScreenClean();
      else if (key === "stage_manager") await api.toggleStageManager();
      else if (key === "lock_screen") await api.lockScreen();
      else if (key === "clear_clipboard") await api.clearClipboard();
      else if (key === "screensaver") await api.startScreensaver();
      else if (key === "close_all_apps") await api.closeAllApps();
      await refresh();
    } catch (e) {
      alert(`操作失败：${e}`);
    } finally {
      setBusy(null);
    }
  }

  async function toggleLaunch() {
    const next = !launch;
    setNotice(null);
    try {
      await api.setLaunchAtLogin(next);
      setLaunch(next);
      setNotice(next ? "已开启开机自启" : "已关闭开机自启");
    } catch (e) {
      setNotice(`自启设置失败：${e}`);
    }
  }

  async function onCheckUpdate() {
    if (!updateEnabled) {
      setUpdateMsg("更新服务尚未配置（暂未提供线上地址）");
      return;
    }
    setUpdating(true);
    setUpdateMsg("正在检查更新…");
    try {
      const r = await checkAndInstall();
      setUpdateMsg(r.message);
    } catch (e) {
      setUpdateMsg(`更新失败：${e}`);
    } finally {
      setUpdating(false);
    }
  }

  return (
    <div className="flex min-h-full w-full flex-col rounded-2xl border border-white/10 bg-zinc-900/70 p-3 text-white shadow-2xl backdrop-blur-xl">
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-[15px] font-semibold tracking-wide">
            SwitchKit
          </span>
          {sidecar === true && (
            <span className="rounded bg-white/10 px-1.5 py-0.5 text-[9px] text-white/60">
              sidecar
            </span>
          )}
        </div>
        <span className="text-[10px] text-white/40">v{APP_VERSION}</span>
      </div>

      <ToggleGrid states={states} busyKey={busy} onToggle={onToggle} />
      <ResourcePanel />
      <BluetoothSection />
      <DeepSeekSection
        version={dsVersion}
        onChanged={() => setDsVersion((v) => v + 1)}
      />

      {showSettings && (
        <SettingsSection
          version={APP_VERSION}
          launch={launch}
          onToggleLaunch={toggleLaunch}
          updating={updating}
          updateMsg={updateMsg}
          onCheckUpdate={onCheckUpdate}
          dsVersion={dsVersion}
          notice={notice}
          onDeepSeekChanged={() => setDsVersion((v) => v + 1)}
          onClose={() => {
            setShowSettings(false);
            setNotice(null);
          }}
        />
      )}

      <div className="mt-3 flex items-center justify-between border-t border-white/10 pt-2">
        <div className="flex items-center gap-3 text-[11px] text-white/50">
          <button
            onClick={() => setShowSettings((v) => !v)}
            className={
              "flex items-center gap-1 rounded px-2 py-0.5 transition hover:bg-white/10 " +
              (showSettings ? "bg-white/10 text-white/85" : "text-white/60 hover:text-white/90")
            }
          >
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            设置
          </button>
          <span className="flex items-center gap-1.5">
            <StatusDot on={sidecar === true} /> 原生守护
          </span>
        </div>
        <button
          onClick={async () => {
            try {
              await api.quitApp();
            } catch {
              /* ignore */
            }
          }}
          className="text-[11px] text-white/50 transition hover:text-white/80"
        >
          退出
        </button>
      </div>
    </div>
  );
}
