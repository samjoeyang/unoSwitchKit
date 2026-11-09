import { useEffect, useMemo, useState } from "react";
import type {
  BundleUsage,
  CleanItem,
  CleanPreview,
  CleanResult,
  CleanRisk,
  LargeFile,
  Settings,
  TrashItem,
} from "../types";
import { api } from "../ipc";

function fmtBytes(b: number): string {
  if (b > 1024 * 1024 * 1024) return `${(b / 1024 / 1024 / 1024).toFixed(2)} GB`;
  if (b > 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)} MB`;
  if (b > 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${b} B`;
}

/// 分类展示顺序（决定 UI 分组先后）。
const CATEGORY_ORDER = [
  "browser",
  "system",
  "syscache",
  "app",
  "dev",
  "xcode",
  "logs",
  "ios",
  "downloads",
  "leftover",
  "custom",
  "ai",
  "mail",
];

const RISK_META: Record<CleanRisk, { label: string; cls: string }> = {
  safe: { label: "安全", cls: "bg-emerald-500/15 text-emerald-300" },
  caution: { label: "谨慎", cls: "bg-amber-500/15 text-amber-300" },
  danger: { label: "危险", cls: "bg-rose-500/15 text-rose-300" },
};

/// 单行路径：过长截断，悬浮显示完整路径。
function PathRow({ path, size }: { path: string; size: string }) {
  return (
    <div className="flex min-w-0 items-center justify-between gap-2">
      <span className="min-w-0 flex-1 truncate text-white/55" title={path}>
        {path}
      </span>
      <span className="shrink-0 text-white/50">{size}</span>
    </div>
  );
}

export function CleanupPanel() {
  const [tab, setTab] = useState<"clean" | "analysis" | "schedule" | "trash">("clean");
  const [preview, setPreview] = useState<CleanPreview | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [msg, setMsg] = useState<string>("");

  // 清理执行态与结果：独立于 msg，清理后的静默重扫不会把它们清空
  const [cleaning, setCleaning] = useState(false);
  const [result, setResult] = useState<CleanResult | null>(null);
  const [resultError, setResultError] = useState<string>("");
  const [showFailed, setShowFailed] = useState(false);

  // 分析
  const [bundles, setBundles] = useState<BundleUsage[] | null>(null);
  const [large, setLarge] = useState<LargeFile[] | null>(null);
  const [analysisBusy, setAnalysisBusy] = useState(false);
  const [threshold, setThreshold] = useState(500);
  const [pendingLarge, setPendingLarge] = useState<string | null>(null);
  const [largeBusy, setLargeBusy] = useState<string | null>(null);

  // 定时
  const [settings, setSettings] = useState<Settings | null>(null);
  const [schedBusy, setSchedBusy] = useState(false);

  // 回收站（本应用移入废纸篓的项）
  const [trashItems, setTrashItems] = useState<TrashItem[] | null>(null);
  const [trashBusy, setTrashBusy] = useState(false);

  /// silent=true 用于清理后重扫：保留结果卡，不清空提示。
  async function scanCleanup(silent = false) {
    setBusy("scan");
    if (!silent) setMsg("");
    try {
      const p = await api.scanCleanup();
      setPreview(p);
      // 默认勾选所有「安全」且存在的项（Xcode/模拟器体积大，默认不勾选，可手动选）
      const def = new Set(
        p.items
          .filter(
            (i) => i.exists && i.risk === "safe" && i.category !== "xcode"
          )
          .map((i) => i.id)
      );
      setSelected(def);
    } catch (e) {
      setMsg(`扫描失败：${e}`);
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    scanCleanup();
  }, []);
  useEffect(() => {
    (async () => {
      try {
        setSettings(await api.getSettings());
      } catch {
        /* ignore */
      }
    })();
  }, []);

  const groups = useMemo(() => {
    if (!preview) return [] as { category: string; label: string; items: CleanItem[] }[];
    const map = new Map<string, { label: string; items: CleanItem[] }>();
    for (const it of preview.items) {
      if (!it.exists) continue;
      const g =
        map.get(it.category) ?? { label: it.category_label, items: [] };
      g.items.push(it);
      map.set(it.category, g);
    }
    return CATEGORY_ORDER.filter((c) => map.has(c)).map((c) => ({
      category: c,
      ...map.get(c)!,
    }));
  }, [preview]);

  const selectedBytes = useMemo(() => {
    if (!preview) return 0;
    return preview.items
      .filter((i) => selected.has(i.id))
      .reduce((a, i) => a + i.size_bytes, 0);
  }, [preview, selected]);

  function toggle(id: string) {
    setConfirmClear(false);
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  async function doClean() {
    if (cleaning || busy === "scan") return;
    if (selected.size === 0) {
      setMsg("请先勾选要清理的项");
      return;
    }
    if (!confirmClear) {
      setConfirmClear(true);
      setResult(null);
      setResultError("");
      const selItems = preview
        ? preview.items.filter((i) => selected.has(i.id))
        : [];
      const dangerCount = selItems.filter((i) => i.risk === "danger").length;
      const cautionCount = selItems.filter((i) => i.risk === "caution").length;
      let warn = "";
      if (dangerCount > 0) {
        warn = `含 ${dangerCount} 项危险项（如 iOS 备份、应用残留），`;
      } else if (cautionCount > 0) {
        warn = `含 ${cautionCount} 项谨慎项（如 App 缓存），`;
      }
      setMsg(
        `将${warn}清理 ${selected.size} 项（约 ${fmtBytes(
          selectedBytes
        )}）。先移入废纸篓、可恢复，再次点击确认。`
      );
      return;
    }
    const ids = [...selected];
    setConfirmClear(false);
    setCleaning(true);
    setResult(null);
    setResultError("");
    setShowFailed(false);
    setMsg("");
    try {
      const r = await api.cleanSelected(ids);
      setResult(r);
    } catch (e) {
      setResultError(`清理失败：${e}`);
    } finally {
      setCleaning(false);
      // 结果卡独立于 msg；静默重扫只刷新列表，不覆盖结果提醒
      await scanCleanup(true);
    }
  }

  async function loadAnalysis() {
    setAnalysisBusy(true);
    setMsg("");
    try {
      const [b, l] = await Promise.all([
        api.bundleUsage(),
        api.largeFiles(threshold),
      ]);
      setBundles(b);
      setLarge(l);
    } catch (e) {
      setMsg(`分析失败：${e}`);
    } finally {
      setAnalysisBusy(false);
    }
  }

  useEffect(() => {
    if (tab === "analysis" && !bundles && !large) loadAnalysis();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  async function deleteLarge(f: LargeFile) {
    if (pendingLarge !== f.id) {
      setPendingLarge(f.id);
      return;
    }
    setPendingLarge(null);
    setLargeBusy(f.id);
    try {
      await api.trashPath(f.path);
      setMsg(`已移入废纸篓：${f.path}`);
      await loadAnalysis();
    } catch (e) {
      setMsg(`清理失败：${e}`);
    } finally {
      setLargeBusy(null);
    }
  }

  async function saveSchedule(patch: Partial<Settings>) {
    if (!settings) return;
    setSchedBusy(true);
    try {
      const next = { ...settings, ...patch };
      await api.saveSettings(next);
      setSettings(next);
    } catch (e) {
      setMsg(`保存失败：${e}`);
    } finally {
      setSchedBusy(false);
    }
  }

  // ---------------- 回收站 ----------------

  async function loadTrash() {
    setTrashBusy(true);
    try {
      setTrashItems(await api.listTrashItems());
    } catch (e) {
      setMsg(`加载回收站失败：${e}`);
    } finally {
      setTrashBusy(false);
    }
  }

  useEffect(() => {
    if (tab === "trash" && trashItems === null) loadTrash();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  async function restoreTrash(t: TrashItem) {
    setTrashBusy(true);
    try {
      await api.restoreTrashItem(t.trashed_path);
      setMsg(`已恢复：${t.name}`);
      await loadTrash();
    } catch (e) {
      setMsg(`恢复失败：${e}`);
    } finally {
      setTrashBusy(false);
    }
  }

  async function emptyTrashItems() {
    setTrashBusy(true);
    try {
      const n = await api.emptySwitchkitTrash();
      setMsg(`已清空本应用移入废纸篓的 ${n} 项`);
      await loadTrash();
    } catch (e) {
      setMsg(`清空失败：${e}`);
    } finally {
      setTrashBusy(false);
    }
  }

  /// 将清理项加入持久化排除列表（永不出现在清单中）。
  async function excludeItem(it: CleanItem) {
    if (!settings) return;
    if (settings.exclude_paths.includes(it.path)) {
      setMsg(`「${it.label}」已在排除列表`);
      return;
    }
    const next = {
      ...settings,
      exclude_paths: [...settings.exclude_paths, it.path],
    };
    try {
      await api.saveSettings(next);
      setSettings(next);
      setMsg(`已将「${it.label}」加入排除列表`);
      await scanCleanup();
    } catch (e) {
      setMsg(`排除失败：${e}`);
    }
  }

  function trashTime(ts: number): string {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
      d.getHours()
    )}:${pad(d.getMinutes())}`;
  }

  function lastRunText(ts: number): string {
    if (!ts) return "从未";
    const diff = Math.floor(Date.now() / 1000 - ts);
    if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    return `${Math.floor(diff / 86400)} 天前`;
  }

  const tabs: { key: typeof tab; label: string }[] = [
    { key: "clean", label: "清理" },
    { key: "analysis", label: "分析" },
    { key: "schedule", label: "定时" },
    { key: "trash", label: "回收站" },
  ];

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">清理无用文件</span>
        <div className="flex gap-1">
          {tabs.map((t) => (
            <button
              key={t.key}
              onClick={() => setTab(t.key)}
              disabled={cleaning}
              className={
                "rounded px-2 py-0.5 text-[10px] transition disabled:opacity-40 " +
                (tab === t.key
                  ? "bg-white/15 text-white/90"
                  : "text-white/45 hover:text-white/75")
              }
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {/* ---------------- 清理 ---------------- */}
      {tab === "clean" && (
        <div className="mt-2">
          <div className="mb-1.5 flex items-center justify-between">
            <button
              onClick={() => scanCleanup()}
              disabled={busy === "scan" || cleaning}
              className="text-[11px] text-white/60 hover:text-white/90 disabled:opacity-40"
            >
              {busy === "scan" ? "扫描中…" : "重新扫描"}
            </button>
            <span className="text-[10px] text-white/45">
              已选 {selected.size} 项 · {fmtBytes(selectedBytes)}
            </span>
          </div>

          <div className="relative">
            {cleaning && (
              <div className="cleanup-progress absolute inset-x-0 top-0 z-10 h-[3px] overflow-hidden rounded-full bg-indigo-400/15" />
            )}
            {cleaning && (
              <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-lg bg-black/25 backdrop-blur-[1px]">
                <div className="flex items-center gap-1.5 rounded-md bg-black/55 px-2.5 py-1 text-[10px] text-white/90">
                  <span className="cleanup-spinner inline-block h-3 w-3 rounded-full border-2 border-white/25 border-t-indigo-300" />
                  正在移入废纸篓…
                </div>
              </div>
            )}
            <div className="thin-scroll max-h-44 space-y-2 overflow-y-auto pr-1">
            {groups.length === 0 && !busy && !cleaning && (
              <div className="text-[11px] text-white/45">暂无可清理项</div>
            )}
            {groups.map((g) => {
              const allSelected =
                g.items.every((i) => selected.has(i.id)) && g.items.length > 0;
              return (
                <div key={g.category}>
                  <div className="flex items-center justify-between">
                    <span className="text-[11px] font-medium text-white/75">
                      {g.label}
                    </span>
                    <button
                      onClick={() => {
                        setConfirmClear(false);
                        setSelected((prev) => {
                          const n = new Set(prev);
                          if (allSelected)
                            g.items.forEach((i) => n.delete(i.id));
                          else g.items.forEach((i) => n.add(i.id));
                          return n;
                        });
                      }}
                      className="text-[10px] text-white/40 hover:text-white/70"
                    >
                      {allSelected ? "全不选" : "全选"}
                    </button>
                  </div>
                  <div className="mt-0.5 space-y-0.5">
                    {g.items.map((it) => {
                      const risk = RISK_META[it.risk];
                      const on = selected.has(it.id);
                      return (
                        <button
                          key={it.id}
                          onClick={() => toggle(it.id)}
                          className="flex w-full items-center gap-2 rounded px-1 py-0.5 text-left transition hover:bg-white/5"
                        >
                          <span
                            className={
                              "flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-[4px] border " +
                              (on
                                ? "border-indigo-400 bg-indigo-500/70"
                                : "border-white/25")
                            }
                          >
                            {on && (
                              <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="4" strokeLinecap="round" strokeLinejoin="round">
                                <path d="M20 6 9 17l-5-5" />
                              </svg>
                            )}
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="flex min-w-0 items-start gap-1.5">
                              <span className="min-w-0 break-all text-[11px] leading-snug text-white/80">
                                {it.label}
                              </span>
                              <span
                                className={
                                  "shrink-0 rounded px-1 text-[9px] leading-snug " +
                                  risk.cls
                                }
                              >
                                {risk.label}
                              </span>
                            </span>
                          </span>
                          <span className="flex shrink-0 items-center gap-1 self-start">
                            <span className="text-[10px] text-white/50">
                              {fmtBytes(it.size_bytes)}
                            </span>
                            <span
                              role="button"
                              title="加入排除列表（永不清理）"
                              onClick={(e) => {
                                e.stopPropagation();
                                excludeItem(it);
                              }}
                              className="cursor-pointer rounded px-1 text-[11px] text-white/30 hover:text-rose-300"
                            >
                              ⊘
                            </span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>
              );
            })}
            </div>
          </div>

          <button
            onClick={doClean}
            disabled={cleaning || busy === "scan" || selected.size === 0}
            className={
              "mt-2 flex w-full items-center justify-center gap-1.5 rounded-lg py-1.5 text-[12px] font-medium transition disabled:opacity-50 " +
              (cleaning
                ? "bg-indigo-500/30 text-indigo-100"
                : confirmClear
                  ? "bg-rose-500/80 text-white hover:bg-rose-500"
                  : "bg-indigo-500/25 text-indigo-100 hover:bg-indigo-500/35")
            }
          >
            {cleaning ? (
              <>
                <span className="cleanup-spinner inline-block h-3 w-3 rounded-full border-2 border-white/25 border-t-white" />
                正在清理…（移入废纸篓）
              </>
            ) : confirmClear ? (
              "确认清理（移入废纸篓）"
            ) : (
              "清理选中"
            )}
          </button>
          <div className="mt-1 text-[10px] leading-snug text-white/40">
            清理项先移入废纸篓，可在「废纸篓」中恢复。谨慎/危险项需手动勾选，绝不自动清理。
          </div>
        </div>
      )}

      {/* ---------------- 分析 ---------------- */}
      {tab === "analysis" && (
        <div className="mt-2">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[10px] text-white/45">空间占用 Top & 大文件</span>
            <button
              onClick={loadAnalysis}
              disabled={analysisBusy}
              className="text-[11px] text-white/60 hover:text-white/90 disabled:opacity-40"
            >
              {analysisBusy ? "分析中…" : "重新分析"}
            </button>
          </div>

          <div className="mb-2 flex items-center gap-2">
            <span className="text-[10px] text-white/45">大文件阈值</span>
            {[200, 500, 1000, 2000].map((v) => (
              <button
                key={v}
                onClick={() => {
                  setThreshold(v);
                  setBundles(null);
                  setLarge(null);
                }}
                className={
                  "rounded px-1.5 py-0.5 text-[10px] transition " +
                  (threshold === v
                    ? "bg-white/15 text-white/85"
                    : "bg-white/5 text-white/45 hover:text-white/70")
                }
              >
                {v >= 1000 ? `${v / 1000} GB` : `${v} MB`}
              </button>
            ))}
          </div>

          <div className="thin-scroll max-h-44 space-y-2 overflow-y-auto pr-1">
            <div>
              <div className="text-[11px] font-medium text-white/75">
                占用排行（按文件夹）
              </div>
              <div className="mt-0.5 space-y-0.5">
                {(bundles ?? []).slice(0, 15).map((b) => (
                  <PathRow
                    key={b.path}
                    path={b.name}
                    size={fmtBytes(b.size_bytes)}
                  />
                ))}
                {(bundles ?? []).length === 0 && (
                  <div className="text-[10px] text-white/40">无数据</div>
                )}
              </div>
            </div>

            <div>
              <div className="text-[11px] font-medium text-white/75">
                大文件 / 疑似重复
              </div>
              <div className="mt-0.5 space-y-0.5">
                {(large ?? []).slice(0, 20).map((f) => (
                  <div
                    key={f.id}
                    className="flex items-center justify-between gap-2"
                  >
                    <span className="min-w-0 flex-1 truncate text-white/55" title={f.path}>
                      {f.path}
                    </span>
                    {f.duplicate && (
                      <span className="shrink-0 rounded bg-amber-500/15 px-1 text-[9px] text-amber-300">
                        疑似重复
                      </span>
                    )}
                    <span className="shrink-0 text-[10px] text-white/50">
                      {fmtBytes(f.size_bytes)}
                    </span>
                    <button
                      onClick={() => deleteLarge(f)}
                      disabled={largeBusy === f.id}
                      className={
                        "shrink-0 rounded px-1.5 py-0.5 text-[9px] transition disabled:opacity-40 " +
                        (pendingLarge === f.id
                          ? "bg-rose-500/70 text-white"
                          : "bg-white/10 text-white/60 hover:bg-white/20")
                      }
                    >
                      {pendingLarge === f.id ? "确认删除" : "移入废纸篓"}
                    </button>
                  </div>
                ))}
                {(large ?? []).length === 0 && (
                  <div className="text-[10px] text-white/40">
                    {analysisBusy ? "分析中…" : "无大文件"}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ---------------- 定时 ---------------- */}
      {tab === "schedule" && settings && (
        <div className="mt-2 space-y-2">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-[12px] text-white/85">自动清理（仅安全项）</div>
              <div className="text-[10px] text-white/40">
                后台定时清理浏览器/系统/开发缓存，上次：
                {lastRunText(settings.last_auto_clean_ts)}
              </div>
            </div>
            <button
              role="switch"
              aria-checked={settings.auto_clean_enabled}
              onClick={() =>
                saveSchedule({ auto_clean_enabled: !settings.auto_clean_enabled })
              }
              disabled={schedBusy}
              className={
                "relative h-[18px] w-[32px] shrink-0 rounded-full transition " +
                (settings.auto_clean_enabled
                  ? "bg-indigo-500/80"
                  : "bg-white/15")
              }
            >
              <span
                className={
                  "absolute top-[2px] h-[14px] w-[14px] rounded-full bg-white shadow transition-all " +
                  (settings.auto_clean_enabled ? "left-[16px]" : "left-[2px]")
                }
              />
            </button>
          </div>

          <div className="flex items-center justify-between">
            <span className="text-[12px] text-white/85">清理周期</span>
            <div className="flex gap-1">
              {[
                { v: 1, label: "每天" },
                { v: 7, label: "每周" },
                { v: 30, label: "每月" },
              ].map((o) => (
                <button
                  key={o.v}
                  onClick={() => saveSchedule({ auto_clean_interval_days: o.v })}
                  className={
                    "rounded px-2 py-0.5 text-[10px] transition " +
                    (settings.auto_clean_interval_days === o.v
                      ? "bg-white/15 text-white/85"
                      : "bg-white/5 text-white/45 hover:text-white/70")
                  }
                >
                  {o.label}
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-[12px] text-white/85">下载过期天数</span>
            <input
              type="number"
              min={1}
              max={365}
              value={settings.downloads_expire_days}
              onChange={(e) =>
                saveSchedule({
                  downloads_expire_days: Math.max(
                    1,
                    Math.min(365, parseInt(e.target.value) || 30)
                  ),
                })
              }
              className="w-16 rounded bg-white/10 px-1.5 py-0.5 text-[11px] text-white/85 outline-none"
            />
          </div>
          <div className="text-[10px] leading-snug text-white/40">
            自动清理只动「安全」级缓存，不会触碰 App 数据、应用残留与文件，且同样先移入废纸篓。
          </div>
        </div>
      )}

      {tab === "trash" && (
        <div className="mt-2">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[10px] text-white/45">本应用移入废纸篓的项</span>
            <button
              onClick={emptyTrashItems}
              disabled={trashBusy}
              className="text-[11px] text-white/60 hover:text-white/90 disabled:opacity-40"
            >
              {trashBusy ? "处理中…" : "清空本应用项"}
            </button>
          </div>
          <div className="thin-scroll max-h-44 space-y-1 overflow-y-auto pr-1">
            {(trashItems ?? []).map((t) => (
              <div key={t.id} className="flex items-center justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <div
                    className="truncate text-[11px] text-white/80"
                    title={t.original_path}
                  >
                    {t.name}
                  </div>
                  <div className="text-[9px] text-white/35">
                    {trashTime(t.moved_at)} · {fmtBytes(t.size_bytes)}
                  </div>
                </div>
                <button
                  onClick={() => restoreTrash(t)}
                  disabled={trashBusy}
                  className="shrink-0 rounded bg-white/10 px-1.5 py-0.5 text-[9px] text-white/65 hover:bg-white/20 disabled:opacity-40"
                >
                  恢复
                </button>
              </div>
            ))}
            {(trashItems ?? []).length === 0 && (
              <div className="text-[10px] text-white/40">
                {trashBusy ? "加载中…" : "暂无本应用移入的项"}
              </div>
            )}
          </div>
          <div className="mt-1 text-[10px] leading-snug text-white/40">
            这里的项来自清理操作；清空才真正释放磁盘空间。也可在 Finder 废纸篓中手动恢复。
          </div>
        </div>
      )}

      {/* 结果提醒：独立于 msg，扫描/重扫都不会清空，需用户手动关闭 */}
      {resultError && (
        <div className="cleanup-result mt-2 flex items-start gap-2 rounded-lg border border-rose-400/30 bg-rose-500/10 px-2.5 py-2">
          <span className="mt-[1px] text-[11px] text-rose-300">✕</span>
          <span className="min-w-0 flex-1 text-[11px] leading-snug text-rose-100">
            {resultError}
          </span>
          <button
            onClick={() => setResultError("")}
            className="shrink-0 text-[10px] text-white/40 hover:text-white/80"
          >
            关闭
          </button>
        </div>
      )}

      {result && (
        <div className="cleanup-result mt-2 rounded-lg border border-emerald-400/25 bg-emerald-400/10 px-2.5 py-2">
          <div className="flex items-start gap-2">
            <span className="mt-[1px] flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-emerald-400/25 text-[10px] text-emerald-200">
              ✓
            </span>
            <div className="min-w-0 flex-1">
            <div className="text-[11px] font-medium text-emerald-200">
              已转移 {fmtBytes(result.freed_bytes)} 至废纸篓
            </div>
            <div className="mt-0.5 text-[10px] leading-snug text-white/60">
              成功 {result.cleaned_count} 项，已移入废纸篓（可恢复；清空废纸篓后才真正释放空间）
              {result.failed.length > 0
                ? ` · ${result.failed.length} 项未能清理`
                : " · 全部成功"}
            </div>
            {result.cleaned_count > 0 && (
              <button
                onClick={emptyTrashItems}
                disabled={trashBusy}
                className="mt-1 rounded bg-white/10 px-1.5 py-0.5 text-[10px] text-white/60 hover:bg-white/20 disabled:opacity-40"
              >
                清空本应用移入的废纸篓项（释放空间）
              </button>
            )}
              {result.failed.length > 0 && (
                <div className="mt-1">
                  <button
                    onClick={() => setShowFailed((v) => !v)}
                    className="text-[10px] text-amber-300/90 hover:text-amber-200"
                  >
                    {showFailed ? "收起" : "查看"}未清理项
                  </button>
                  {showFailed && (
                    <div className="thin-scroll mt-1 max-h-20 space-y-0.5 overflow-y-auto pr-1">
                      {result.failed.map((f) => (
                        <div
                          key={f.id}
                          className="truncate text-[10px] text-white/50"
                          title={f.detail}
                        >
                          · {f.detail || f.id}
                        </div>
                      ))}
                    </div>
                  )}
                  <div className="mt-1 text-[10px] leading-snug text-white/40">
                    多为系统保护目录（TCC）。需清理请到 系统设置 → 隐私与安全性 →
                    完全磁盘访问权限 为本 App 授权后重试。
                  </div>
                </div>
              )}
            </div>
            <button
              onClick={() => setResult(null)}
              className="shrink-0 text-[10px] text-white/40 hover:text-white/80"
            >
              关闭
            </button>
          </div>
        </div>
      )}

      {msg && (
        <div
          className={
            "mt-2 rounded px-2 py-1 text-[11px] leading-snug " +
            (confirmClear
              ? "bg-amber-400/10 text-amber-200"
              : "text-white/60")
          }
        >
          {msg}
        </div>
      )}
    </div>
  );
}
