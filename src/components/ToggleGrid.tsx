import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { StatusDot } from "./StatusDot";
import type { AppearanceMode, ToggleStates } from "../types";

export type ToggleKey =
  | "dark_mode"
  | "system_screenshot"
  | "desktop_hidden"
  | "prevent_sleep"
  | "empty_trash"
  | "hide_dock"
  | "show_hidden_files"
  | "screen_clean"
  | "stage_manager"
  | "lock_screen"
  | "clear_clipboard"
  | "screensaver"
  | "close_all_apps"
  | "flush_dns";

interface Item {
  key: ToggleKey;
  label: string;
  status: (s: ToggleStates) => string;
  active: (s: ToggleStates) => boolean;
  /** 不可见条件（返回 true 则隐藏该项） */
  hidden?: (s: ToggleStates) => boolean;
}

const ITEMS: Item[] = [
  {
    key: "dark_mode",
    label: "外观",
    status: (s) => (s.dark_mode ? "深色" : "浅色"),
    active: (s) => s.dark_mode,
  },
  {
    key: "desktop_hidden",
    label: "隐藏桌面",
    status: (s) => (s.desktop_hidden ? "已隐藏" : "显示中"),
    active: (s) => s.desktop_hidden,
  },
  {
    key: "prevent_sleep",
    label: "阻止睡眠",
    status: (s) => (s.prevent_sleep ? "生效中" : "关闭"),
    active: (s) => s.prevent_sleep,
  },
  {
    key: "empty_trash",
    label: "清空废纸篓",
    status: (s) => `${s.trash_count} 项`,
    active: () => false,
  },
  {
    key: "hide_dock",
    label: "隐藏 Dock",
    status: (s) => (s.hide_dock ? "已隐藏" : "显示中"),
    active: (s) => s.hide_dock,
  },
  {
    key: "show_hidden_files",
    label: "显示隐藏文件",
    status: (s) => (s.show_hidden_files ? "显示中" : "隐藏中"),
    active: (s) => s.show_hidden_files,
  },
  {
    key: "screen_clean",
    label: "屏幕清洁",
    status: (s) => (s.screen_clean ? "已收起窗口" : "一键收起"),
    active: (s) => s.screen_clean,
  },
  {
    key: "stage_manager",
    label: "台前调度",
    status: (s) => (s.stage_manager ? "开" : "关"),
    active: (s) => s.stage_manager,
    // macOS 13 以下不支持，直接不显示
    hidden: (s) => !s.stage_manager_available,
  },
  {
    key: "lock_screen",
    label: "锁定屏幕",
    status: () => "点击锁定",
    active: () => false,
  },
  {
    key: "clear_clipboard",
    label: "清空剪贴板",
    status: () => "点击清空",
    active: () => false,
  },
  {
    key: "screensaver",
    label: "屏幕保护",
    status: () => "点击启动",
    active: () => false,
  },
  {
    key: "close_all_apps",
    label: "关闭所有应用",
    status: () => "逐个退出",
    active: () => false,
  },
  {
    key: "flush_dns",
    label: "刷新 DNS",
    status: () => "点击刷新",
    active: () => false,
  },
  {
    key: "system_screenshot",
    label: "截屏录屏",
    status: () => "打开系统工具栏",
    active: () => false,
  },
];

/** 默认顺序，也是 settings.toggle_order 为空时的顺序。 */
export const ALL_TOGGLE_KEYS: ToggleKey[] = ITEMS.map((it) => it.key);

export const APPEARANCE_TEXT: Record<AppearanceMode, string> = {
  light: "浅色 · 点按切深色",
  dark: "深色 · 点按切自动",
  auto: "自动 · 日出日落",
};

interface Props {
  states: ToggleStates | null;
  busyKey: string | null;
  onToggle: (key: ToggleKey) => void;
  /** 外观三态，独立于全量状态轮询（系统自动切换时也能跟上） */
  appearanceMode?: AppearanceMode | null;
  /** 覆盖某项的状态副标题（用于一次性操作反馈，如「已刷新」） */
  statusOverrides?: Partial<Record<ToggleKey, string>>;
  /** 编辑模式：卡片只响应拖动，不触发开关动作 */
  editMode?: boolean;
  /** 持久化后的顺序（由 useToggleOrder 提供） */
  order?: ToggleKey[];
  /** 拖拽结束后提交新顺序 */
  onReorder?: (next: ToggleKey[]) => void;
}

/** 拖拽把手：右上角六点阵，仅编辑模式出现。 */
function GripIcon({ active }: { active: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill={active ? "#c7d2fe" : "rgba(255,255,255,0.55)"}
      aria-hidden="true"
    >
      <circle cx="4" cy="2" r="1" />
      <circle cx="8" cy="2" r="1" />
      <circle cx="4" cy="6" r="1" />
      <circle cx="8" cy="6" r="1" />
      <circle cx="4" cy="10" r="1" />
      <circle cx="8" cy="10" r="1" />
    </svg>
  );
}

/** 找到指针最近的卡片 key；用矩形中心距离，不依赖 DOM 命中测试。
 *  返回 key 而非 DOM 索引：隐藏项（如 macOS 12 的台前调度）不渲染，
 *  DOM 索引与完整顺序数组的下标会错位。 */
function nearestCardKey(x: number, y: number): ToggleKey | null {
  const cards = document.querySelectorAll<HTMLElement>("[data-toggle-key]");
  let best: ToggleKey | null = null;
  let bestDist = Infinity;
  cards.forEach((el) => {
    const r = el.getBoundingClientRect();
    const d = Math.hypot(x - (r.left + r.width / 2), y - (r.top + r.height / 2));
    if (d < bestDist) {
      bestDist = d;
      best = (el.dataset.toggleKey ?? null) as ToggleKey | null;
    }
  });
  return best;
}

/** 面板内容在 #root 内滚动；拖到上下边缘时自动滚动，才能跨屏换位。 */
function autoScrollRoot(y: number) {
  const root = document.getElementById("root");
  if (!root) return;
  const r = root.getBoundingClientRect();
  if (y < r.top + 44) root.scrollTop -= 14;
  else if (y > r.bottom - 44) root.scrollTop += 14;
}

export function ToggleGrid({
  states,
  busyKey,
  onToggle,
  appearanceMode,
  statusOverrides,
  editMode = false,
  order = ALL_TOGGLE_KEYS,
  onReorder,
}: Props) {
  const [preview, setPreview] = useState<ToggleKey[] | null>(null);
  const [dragKey, setDragKey] = useState<ToggleKey | null>(null);
  const shown = preview ?? order;
  // window 上的 pointermove 回调需要读到最新顺序，用 ref 镜像避免闭包过期。
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const dragRef = useRef<ToggleKey | null>(null);

  function startDrag(key: ToggleKey, e: ReactPointerEvent<HTMLButtonElement>) {
    if (!editMode) return;
    e.preventDefault();
    e.stopPropagation();
    dragRef.current = key;
    setDragKey(key);

    const onMove = (ev: PointerEvent) => {
      const k = dragRef.current;
      if (!k) return;
      autoScrollRoot(ev.clientY);
      const cur = shownRef.current;
      const from = cur.indexOf(k);
      const hit = nearestCardKey(ev.clientX, ev.clientY);
      const to = hit ? cur.indexOf(hit) : -1;
      if (from < 0 || to < 0 || to === from) return;
      const next = [...cur];
      next.splice(from, 1);
      next.splice(to, 0, k);
      shownRef.current = next;
      setPreview(next);
    };
    const onEnd = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onEnd);
      window.removeEventListener("pointercancel", onEnd);
      const next = shownRef.current;
      dragRef.current = null;
      setDragKey(null);
      setPreview(null);
      onReorder?.(next);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onEnd);
    window.addEventListener("pointercancel", onEnd);
  }

  const byKey = new Map(ITEMS.map((it) => [it.key, it]));
  const visibleKeys = new Set(
    (states ? ITEMS.filter((it) => !it.hidden?.(states)) : ITEMS).map((it) => it.key),
  );
  // 隐藏项（如 macOS 12 的台前调度）仍保留在顺序里，换机器时位置不丢。
  const orderedKeys = shown.filter((k) => visibleKeys.has(k));

  return (
    <div className="grid grid-cols-2 gap-2">
      {orderedKeys.map((key) => {
        const it = byKey.get(key);
        if (!it) return null;
        const isAppearance = it.key === "dark_mode" && appearanceMode;
        const status =
          statusOverrides?.[it.key] ??
          (isAppearance
            ? APPEARANCE_TEXT[appearanceMode!]
            : states
              ? it.status(states)
              : "—");
        const active = isAppearance
          ? appearanceMode !== "light"
          : states
            ? it.active(states)
            : false;
        const busy = busyKey === it.key;
        const dragging = dragKey === it.key;
        return (
          <button
            key={it.key}
            data-toggle-key={it.key}
            onPointerDown={(e) => startDrag(it.key, e)}
            onClick={() => {
              if (!editMode) onToggle(it.key);
            }}
            disabled={!editMode && busy}
            style={editMode ? { touchAction: "none" } : undefined}
            className={
              "flex flex-col items-start rounded-xl border bg-white/5 px-3 py-2.5 text-left transition " +
              (editMode
                ? "cursor-grab border-indigo-400/40 ring-1 ring-indigo-400/20 select-none "
                : "border-white/10 hover:bg-white/10 active:scale-[0.98] disabled:opacity-50 ") +
              (dragging ? "opacity-50 " : "")
            }
          >
            <div className="flex w-full items-center justify-between">
              <span className="text-[13px] font-medium text-white/90">
                {it.label}
              </span>
              {editMode ? (
                <GripIcon active={dragging} />
              ) : (
                <StatusDot on={active} />
              )}
            </div>
            <span className="mt-1 text-[11px] text-white/50">
              {busy ? "处理中…" : status}
            </span>
          </button>
        );
      })}
    </div>
  );
}
