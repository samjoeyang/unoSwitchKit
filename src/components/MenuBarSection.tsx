import { useCallback, useEffect, useState } from "react";
import { api } from "../ipc";
import type { MenuBarState } from "../types";

/// 菜单栏图标管理区：默认收纳全部图标，"点亮"的显示在菜单栏。
/// 数据在挂载、面板重新显示、以及开关切换时刷新（刷新即推送巡检策略）。
export function MenuBarSection() {
  const [state, setState] = useState<MenuBarState | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setState(await api.menubarState());
    } catch {
      /* sidecar 未运行时静默 */
    }
  }, []);

  useEffect(() => {
    refresh();
    const onVis = () => {
      if (!document.hidden) refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [refresh]);

  async function toggleItem(id: string, show: boolean) {
    setBusyId(id);
    try {
      await api.menubarSetVisible(id, show);
      await refresh();
    } catch (e) {
      alert(`操作失败：${e}`);
    } finally {
      setBusyId(null);
    }
  }

  async function toggleManaged() {
    if (!state) return;
    const next = !state.managed;
    try {
      await api.menubarSetManaged(next);
      await refresh();
    } catch (e) {
      alert(`操作失败：${e}`);
    }
  }

  if (!state || !state.running) {
    return (
      <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3 text-[11px] text-white/40">
        菜单栏管理：原生守护未运行
      </div>
    );
  }

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">菜单栏图标</span>
        <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-white/60">
          <input
            type="checkbox"
            checked={state.managed}
            onChange={toggleManaged}
            className="accent-indigo-400"
          />
          管理
        </label>
      </div>

      {!state.ax_trusted && (
        <div className="mt-2 rounded-lg bg-amber-500/10 px-2 py-1.5 text-[10px] leading-relaxed text-amber-200/80">
          需要「辅助功能」权限：系统设置 → 隐私与安全性 → 辅助功能，勾选
          switchkit-daemon 后重试。
        </div>
      )}

      {state.managed && state.items.length > 0 && (
        <div className="mt-2 max-h-40 space-y-1 overflow-y-auto pr-1">
          {state.items.map((it) => (
            <div
              key={it.id}
              className="flex items-center justify-between gap-2 rounded-lg px-2 py-1.5 hover:bg-white/5"
            >
              <div className="min-w-0">
                <div className="truncate text-[12px] text-white/85">
                  {it.title || "(无标题)"}
                </div>
                <div className="truncate text-[10px] text-white/40">
                  {it.owner}
                  {it.show ? "" : it.visible ? " · 待收纳" : " · 已收纳"}
                </div>
              </div>
              <button
                onClick={() => toggleItem(it.id, !it.show)}
                disabled={busyId === it.id}
                className={
                  "shrink-0 rounded-full px-2.5 py-0.5 text-[10px] transition disabled:opacity-50 " +
                  (it.show
                    ? "bg-emerald-500/20 text-emerald-200 hover:bg-emerald-500/30"
                    : "bg-white/10 text-white/50 hover:bg-white/20")
                }
              >
                {busyId === it.id ? "…" : it.show ? "已点亮" : "已收纳"}
              </button>
            </div>
          ))}
        </div>
      )}

      {state.managed && state.items.length === 0 && state.ax_trusted && (
        <div className="mt-2 text-[10px] text-white/40">
          未发现可管理的图标
        </div>
      )}
    </div>
  );
}
