import { useState } from "react";
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

export function ResourcePanel() {
  const [busy, setBusy] = useState<string | null>(null);
  const [cmp, setCmp] = useState<MemoryComparison | null>(null);
  const [preview, setPreview] = useState<CachePreview | null>(null);
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
      setMsg("请先点击预览");
      return;
    }
    const dirs = preview.items.filter((i) => i.exists).length;
    if (!confirm(`确认清理 ${dirs} 个缓存目录（约 ${fmtBytes(preview.total_bytes)}）？`))
      return;
    setBusy("cache-c");
    setMsg("");
    try {
      const freed = await api.clearCache();
      setMsg(`已清理缓存：${fmtBytes(freed)}`);
      setPreview(null);
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
              className="text-[11px] text-rose-300 hover:text-rose-200 disabled:opacity-40"
            >
              清理
            </button>
          </div>
        </div>
        {preview && (
          <div className="mt-2 max-h-32 space-y-1 overflow-y-auto text-[11px] text-white/55">
            {preview.items
              .filter((i) => i.exists)
              .map((i) => (
                <div key={i.path} className="flex justify-between gap-2">
                  <span className="truncate">{i.path}</span>
                  <span className="shrink-0">{fmtBytes(i.size_bytes)}</span>
                </div>
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
