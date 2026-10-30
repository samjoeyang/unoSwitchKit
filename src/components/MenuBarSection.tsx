import { useCallback, useEffect, useState } from "react";
import { api } from "../ipc";
import type { MenuBarState } from "../types";

/// 菜单栏图标收纳区（Hidden Bar 同款「分隔条撑开」方案）：
/// 守护进程在菜单栏放置「箭头 + 分隔条」，收起时分隔条撑开、把分界线
/// 左侧的图标全部挤出屏幕；展开时缩回。列表为只读展示，收纳边界由
/// 用户按住 ⌘ 拖动箭头/分隔条自定义。
export function MenuBarSection() {
  const [state, setState] = useState<MenuBarState | null>(null);
  const [busy, setBusy] = useState(false);

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

  async function toggleCollapsed() {
    if (!state) return;
    setBusy(true);
    try {
      const applied = await api.menubarSetCollapsed(!state.collapsed);
      setState({ ...state, collapsed: applied });
      await refresh();
    } catch (e) {
      alert(`操作失败：${e}`);
    } finally {
      setBusy(false);
    }
  }

  if (!state || !state.running) {
    return (
      <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3 text-[11px] text-white/40">
        菜单栏收纳：原生守护未运行
      </div>
    );
  }

  const hiddenCount = state.items.filter((it) => !it.visible).length;

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">菜单栏图标</span>
        <button
          onClick={toggleCollapsed}
          disabled={busy}
          className={
            "rounded-full px-2.5 py-0.5 text-[10px] transition disabled:opacity-50 " +
            (state.collapsed
              ? "bg-emerald-500/20 text-emerald-200 hover:bg-emerald-500/30"
              : "bg-white/10 text-white/60 hover:bg-white/20")
          }
        >
          {busy ? "…" : state.collapsed ? "展开图标" : "收起图标"}
        </button>
      </div>

      <div className="mt-1.5 text-[10px] leading-relaxed text-white/40">
        {state.collapsed
          ? `已收纳 ${hiddenCount} 个图标 · 点菜单栏上的 › 箭头可展开`
          : "首次使用：按住 ⌘ 把菜单栏上的 ‹ 箭头拖到想收纳的图标右侧，再点「收起图标」"}
        <br />
        想常驻显示的图标，按住 ⌘ 拖到箭头右侧即可（拖动一次永久记住）。
      </div>

      {!state.ax_trusted && (
        <div className="mt-2 rounded-lg bg-amber-500/10 px-2 py-1.5 text-[10px] leading-relaxed text-amber-200/80">
          需要「辅助功能」权限（用于显示图标列表）：系统设置 → 隐私与安全性 →
          辅助功能，勾选 switchkit-daemon 后重试。
        </div>
      )}

      {state.ax_trusted && state.items.length > 0 && (
        <div className="mt-2 max-h-40 space-y-1 overflow-y-auto pr-1">
          {state.items.map((it) => (
            <div
              key={it.id}
              className="flex items-center justify-between gap-2 rounded-lg px-2 py-1"
            >
              <div className="min-w-0">
                <div className="truncate text-[12px] text-white/85">
                  {it.title || "(无标题)"}
                </div>
                <div className="truncate text-[10px] text-white/40">{it.owner}</div>
              </div>
              <span
                className={
                  "shrink-0 rounded-full px-2 py-0.5 text-[10px] " +
                  (it.visible
                    ? "bg-white/10 text-white/50"
                    : "bg-indigo-500/20 text-indigo-200")
                }
              >
                {it.visible ? "显示中" : "已收纳"}
              </span>
            </div>
          ))}
        </div>
      )}

      {state.ax_trusted && state.items.length === 0 && (
        <div className="mt-2 text-[10px] text-white/40">未发现菜单栏图标</div>
      )}
    </div>
  );
}
