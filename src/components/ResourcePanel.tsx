import { useState } from "react";
import type { MemoryComparison } from "../types";
import { api } from "../ipc";

function fmtMB(mb: number) {
  return `${(mb / 1024).toFixed(2)} GB`;
}

export function ResourcePanel() {
  const [busy, setBusy] = useState<string | null>(null);
  const [cmp, setCmp] = useState<MemoryComparison | null>(null);
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
