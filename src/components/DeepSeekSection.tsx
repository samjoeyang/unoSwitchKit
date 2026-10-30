import { useCallback, useEffect, useState } from "react";
import { StatusDot } from "./StatusDot";
import { api } from "../ipc";
import type { DeepSeekAccountBalance, DeepSeekKey } from "../types";

/// 定时刷新间隔：60 秒
const REFRESH_MS = 60_000;

function currencySymbol(c: string) {
  return c === "USD" ? "$" : c === "CNY" ? "¥" : `${c} `;
}

/// DeepSeek 余额区：支持多个 API Key；无 Key 时不显示余额（仅留极简添加入口）。
export function DeepSeekSection() {
  const [loaded, setLoaded] = useState(false);
  const [keys, setKeys] = useState<DeepSeekKey[]>([]);
  const [balances, setBalances] = useState<DeepSeekAccountBalance[]>([]);
  const [loading, setLoading] = useState(false);
  const [adding, setAdding] = useState(false);
  const [label, setLabel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [msg, setMsg] = useState("");

  // 初始化：读取已保存的 Keys
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setKeys(s.deepseek_keys ?? []);
      } catch {
        /* ignore */
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  const refreshBalances = useCallback(async () => {
    if (keys.length === 0) {
      setBalances([]);
      return;
    }
    setLoading(true);
    try {
      setBalances(await api.getDeepSeekBalances());
    } catch (e) {
      setMsg(`余额查询失败：${e}`);
    } finally {
      setLoading(false);
    }
  }, [keys.length]);

  // 有 Key 时：立即查一次 + 每 60s 定时刷新 + 面板重新可见时刷新
  useEffect(() => {
    if (keys.length === 0) {
      setBalances([]);
      return;
    }
    refreshBalances();
    const t = setInterval(refreshBalances, REFRESH_MS);
    const onVis = () => {
      if (!document.hidden) refreshBalances();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [keys.length, refreshBalances]);

  async function persist(next: DeepSeekKey[]) {
    const s = await api.getSettings();
    s.deepseek_keys = next;
    await api.saveSettings(s);
    setKeys(next);
  }

  async function addKey() {
    const key = apiKey.trim();
    if (!key) {
      setMsg("请填写 API Key");
      return;
    }
    const next = [
      ...keys,
      {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        label: label.trim() || `Key ${keys.length + 1}`,
        api_key: key,
      },
    ];
    try {
      await persist(next);
      setLabel("");
      setApiKey("");
      setAdding(false);
      setMsg("已添加，正在查询余额…");
    } catch (e) {
      setMsg(`保存失败：${e}`);
    }
  }

  async function removeKey(id: string) {
    try {
      await persist(keys.filter((k) => k.id !== id));
      setMsg("已删除");
    } catch (e) {
      setMsg(`删除失败：${e}`);
    }
  }

  if (!loaded) return null;

  // 无 Key：不显示余额区，仅保留一个极简添加入口
  if (keys.length === 0 && !adding) {
    return (
      <div className="mt-3 flex justify-end">
        <button
          onClick={() => setAdding(true)}
          className="text-[10px] text-white/30 transition hover:text-white/60"
        >
          + DeepSeek 余额
        </button>
      </div>
    );
  }

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">
          DeepSeek 余额
        </span>
        <div className="flex items-center gap-3 text-[10px]">
          <button
            onClick={refreshBalances}
            disabled={loading || keys.length === 0}
            className="text-white/50 transition hover:text-white/85 disabled:opacity-40"
          >
            {loading ? "刷新中…" : "刷新"}
          </button>
          <button
            onClick={() => setAdding((v) => !v)}
            className="text-white/50 transition hover:text-white/85"
          >
            添加
          </button>
        </div>
      </div>

      <div className="mt-2 space-y-1.5">
        {keys.length > 0 && balances.length === 0 && (
          <div className="text-[11px] text-white/40">查询中…</div>
        )}
        {balances.map((b) => (
          <div key={b.id} className="rounded-lg bg-black/20 px-2 py-1.5">
            <div className="flex items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <StatusDot on={b.ok && b.is_available} />
                <span className="truncate text-[12px] text-white/85">
                  {b.label}
                </span>
              </div>
              <button
                onClick={() => removeKey(b.id)}
                className="shrink-0 text-[10px] text-rose-300/60 transition hover:text-rose-300"
              >
                删除
              </button>
            </div>
            {b.ok ? (
              <div className="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5 pl-4 text-[11px]">
                {b.balances.length === 0 && (
                  <span className="text-white/45">无余额信息</span>
                )}
                {b.balances.map((x) => (
                  <span key={x.currency} className="text-white/65">
                    {currencySymbol(x.currency)}
                    {x.total_balance}
                    <span className="text-white/30">
                      {" "}
                      · 赠送 {currencySymbol(x.currency)}
                      {x.granted_balance}
                    </span>
                  </span>
                ))}
                {!b.is_available && (
                  <span className="text-amber-300/80">余额不足</span>
                )}
              </div>
            ) : (
              <div className="mt-0.5 pl-4 text-[10px] text-rose-300/80">
                {b.error}
              </div>
            )}
          </div>
        ))}
      </div>

      {adding && (
        <div className="mt-2 space-y-1.5 border-t border-white/10 pt-2">
          <input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="备注（可选，如：主账号）"
            className="w-full rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px] text-white/85 placeholder:text-white/30 focus:border-indigo-400/50 focus:outline-none"
          />
          <input
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            type="password"
            placeholder="sk-... API Key"
            className="w-full rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px] text-white/85 placeholder:text-white/30 focus:border-indigo-400/50 focus:outline-none"
          />
          <div className="flex items-center justify-between">
            <span className="text-[9px] text-white/30">
              仅保存在本机配置文件中
            </span>
            <div className="flex gap-2">
              <button
                onClick={() => {
                  setAdding(false);
                  setLabel("");
                  setApiKey("");
                }}
                className="text-[10px] text-white/45 transition hover:text-white/70"
              >
                取消
              </button>
              <button
                onClick={addKey}
                className="rounded bg-indigo-500/20 px-2 py-0.5 text-[10px] text-indigo-100 transition hover:bg-indigo-500/30"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}

      {msg && <div className="mt-1.5 text-[10px] text-white/40">{msg}</div>}
    </div>
  );
}
