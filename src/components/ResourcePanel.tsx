import { useEffect, useRef, useState } from "react";
import type { CachePreview, MemoryComparison } from "../types";
import { api } from "../ipc";

function fmtMB(mb: number) {
  return `${(mb / 1024).toFixed(2)} GB`;
}
function fmtBytes(b: number) {
  if (b > 1024 * 1024 * 1024) return `${(b / 1024 / 1024 / 1024).toFixed(2)} GB`;
  if (b > 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)} MB`;
  if (b > 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${b} B`;
}

/// 单行缓存路径：路径过长时，鼠标悬浮其上缓慢向左滚动直到完整显示。
function CachePathRow({ path, size }: { path: string; size: string }) {
  const innerRef = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(false);
  const [shift, setShift] = useState(0);
  const [dur, setDur] = useState(4);

  useEffect(() => {
    const el = innerRef.current;
    const outer = el?.parentElement;
    if (!el || !outer) return;
    const overflowPx = el.scrollWidth - outer.clientWidth;
    if (overflowPx > 1) {
      setOverflow(true);
      setShift(overflowPx);
      // 约 40px/秒，保证长路径也「缓慢」滚动
      setDur(Math.max(2, overflowPx / 40));
    } else {
      setOverflow(false);
    }
  }, [path]);

  return (
    <div className="flex justify-between gap-2">
      <span className="min-w-0 flex-1 overflow-hidden">
        <span
          ref={innerRef}
          className={`inline-block ${
            overflow ? "path-marquee is-overflow whitespace-nowrap" : "truncate"
          }`}
          style={
            overflow
              ? ({
                  "--marquee-shift": `${shift}px`,
                  "--marquee-dur": `${dur}s`,
                } as React.CSSProperties)
              : undefined
          }
        >
          {path}
        </span>
      </span>
      <span className="shrink-0">{size}</span>
    </div>
  );
}

export function ResourcePanel() {
  const [busy, setBusy] = useState<string | null>(null);
  const [cmp, setCmp] = useState<MemoryComparison | null>(null);
  const [preview, setPreview] = useState<CachePreview | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [msg, setMsg] = useState<string>("");

  async function release() {
    setBusy("mem");
    setMsg("");
    try {
      const r = await api.releaseMemory();
      setCmp(r);
      setMsg(
        `内存已释放：使用 ${fmtMB(r.before.used_mb)} → ${fmtMB(r.after.used_mb)}`
      );
    } catch (e) {
      setMsg(`释放失败：${e}`);
    } finally {
      setBusy(null);
    }
  }
  async function flush() {
    setBusy("dns");
    setMsg("");
    try {
      await api.flushDns();
      setMsg("DNS 缓存已刷新");
    } catch (e) {
      setMsg(`刷新失败：${e}`);
    } finally {
      setBusy(null);
    }
  }
  async function previewCache() {
    setConfirmClear(false);
    setBusy("cache-p");
    setMsg("");
    try {
      setPreview(await api.previewCacheCleanup());
    } catch (e) {
      setMsg(`预览失败：${e}`);
    } finally {
      setBusy(null);
    }
  }
  async function clearCache() {
    if (!preview) {
      setMsg("请先点击「预览」");
      return;
    }
    const dirs = preview.items.filter((i) => i.exists).length;
    // 两步确认：首次点击进入待确认态，再次点击才真正执行。
    // 注意：Tauri 的 webview 基于 WKWebView，原生 window.confirm 被静默禁用
    // （不弹窗且恒返回 false），故此处用前端状态实现二次确认，避免点击无反应。
    if (!confirmClear) {
      setConfirmClear(true);
      setMsg(
        `将清理 ${dirs} 个缓存目录（约 ${fmtBytes(preview.total_bytes)}），再次点击「清理」确认`
      );
      return;
    }
    setConfirmClear(false);
    setBusy("cache-c");
    setMsg("");
    try {
      const r = await api.clearCache();
      const parts = [`已清理 ${fmtBytes(r.freed_bytes)}（${r.cleared_count} 个目录）`];
      if (r.failed.length > 0) {
        parts.push(
          `${r.failed.length} 个目录受系统保护未能清理（如 Safari，需在 系统设置 → 隐私与安全性 → 完全磁盘访问权限 中授予后重试）`
        );
      }
      setMsg(parts.join("；"));
      // 清理后自动刷新预览，直观显示剩余大小变化
      setPreview(await api.previewCacheCleanup());
    } catch (e) {
      setMsg(`清理失败：${e}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="mt-3 space-y-2">
      <div className="grid grid-cols-2 gap-2">
        <button
          onClick={release}
          disabled={busy === "mem"}
          className="rounded-xl border border-white/10 bg-indigo-500/15 px-3 py-2 text-[13px] font-medium text-indigo-100 hover:bg-indigo-500/25 disabled:opacity-50"
        >
          {busy === "mem" ? "释放中…" : "释放内存"}
        </button>
        <button
          onClick={flush}
          disabled={busy === "dns"}
          className="rounded-xl border border-white/10 bg-sky-500/15 px-3 py-2 text-[13px] font-medium text-sky-100 hover:bg-sky-500/25 disabled:opacity-50"
        >
          {busy === "dns" ? "刷新中…" : "刷新 DNS"}
        </button>
      </div>

      <div className="rounded-xl border border-white/10 bg-white/5 p-3">
        <div className="flex items-center justify-between">
          <span className="text-[13px] font-medium text-white/90">清理缓存</span>
          <div className="flex gap-3">
            <button
              onClick={previewCache}
              disabled={busy === "cache-p"}
              className="text-[11px] text-white/60 hover:text-white/90 disabled:opacity-40"
            >
              预览
            </button>
            <button
              onClick={clearCache}
              disabled={busy === "cache-c" || !preview}
              className={
                confirmClear
                  ? "rounded px-1 text-[11px] font-semibold text-rose-300 ring-1 ring-rose-400/60"
                  : "text-[11px] text-rose-300 hover:text-rose-200 disabled:opacity-40"
              }
            >
              {confirmClear ? "确认清理" : "清理"}
            </button>
          </div>
        </div>
        {preview && (
          <div className="mt-2 max-h-32 space-y-1 overflow-y-auto text-[11px] text-white/55">
            {preview.items
              .filter((i) => i.exists)
              .map((i) => (
                <CachePathRow
                  key={i.path}
                  path={i.path}
                  size={fmtBytes(i.size_bytes)}
                />
              ))}
            <div className="pt-1 text-right text-white/70">
              合计：{fmtBytes(preview.total_bytes)}
            </div>
          </div>
        )}
      </div>

      {cmp && (
        <div className="rounded-xl border border-white/10 bg-white/5 p-3 text-[11px] text-white/70">
          <div className="flex justify-between">
            <span>内存使用</span>
            <span>
              {fmtMB(cmp.before.used_mb)} → {fmtMB(cmp.after.used_mb)}
            </span>
          </div>
        </div>
      )}
      {msg && <div className="text-[11px] text-white/60">{msg}</div>}
    </div>
  );
}
