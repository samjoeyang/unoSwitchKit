import { useEffect, useState } from "react";
import { ToggleGrid, type ToggleKey } from "./components/ToggleGrid";
import { ResourcePanel } from "./components/ResourcePanel";
import { MenuBarSection } from "./components/MenuBarSection";
import { BluetoothSection } from "./components/BluetoothSection";
import { StatusDot } from "./components/StatusDot";
import { api } from "./ipc";
import type { ToggleStates } from "./types";

export default function App() {
  const [states, setStates] = useState<ToggleStates | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [launch, setLaunch] = useState(false);
  const [sidecar, setSidecar] = useState<boolean | null>(null);

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

  useEffect(() => {
    refresh();
    loadLaunch();
    loadSidecar();
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
    try {
      await api.setLaunchAtLogin(next);
      setLaunch(next);
    } catch (e) {
      alert(`自启设置失败：${e}`);
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
        <span className="text-[10px] text-white/40">v0.3.1</span>
      </div>

      <ToggleGrid states={states} busyKey={busy} onToggle={onToggle} />
      <ResourcePanel />
      <MenuBarSection />
      <BluetoothSection />

      <div className="mt-3 flex items-center justify-between border-t border-white/10 pt-2">
        <label className="flex cursor-pointer items-center gap-2 text-[12px] text-white/70">
          <input
            type="checkbox"
            checked={launch}
            onChange={toggleLaunch}
            className="accent-indigo-400"
          />
          开机自启
        </label>
        <div className="flex items-center gap-3 text-[11px] text-white/50">
          <span className="flex items-center gap-1.5">
            <StatusDot on={sidecar === true} /> 原生守护
          </span>
          <button
            onClick={async () => {
              try {
                await api.quitApp();
              } catch {
                /* ignore */
              }
            }}
            className="transition hover:text-white/80"
          >
            退出
          </button>
        </div>
      </div>
    </div>
  );
}
