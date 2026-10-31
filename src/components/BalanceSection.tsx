import { useCallback, useEffect, useState } from "react";
import { StatusDot } from "./StatusDot";
import { KeyForm } from "./KeyForm";
import { api } from "../ipc";
import { providerName } from "../providers";
import type { AccountBalance, ProviderKey } from "../types";

/// 定时刷新间隔：300 秒
const REFRESH_MS = 300_000;

function currencySymbol(c: string) {
  return c === "USD" ? "$" : c === "CNY" || c === "" ? "¥" : `${c} `;
}

interface Props {
  /// 外部（设置页）新增 Key 后递增，触发重新读取
  version?: number;
  /// Key 增删后通知外部（设置页同步账号数量）
  onChanged?: () => void;
}

/// 模型余额区：支持多平台、多账号混排。
/// 无账号时整块不显示（首次添加入口在「设置」里）。
export function BalanceSection({ version = 0, onChanged }: Props) {
  const [loaded, setLoaded] = useState(false);
  const [keys, setKeys] = useState<ProviderKey[]>([]);
  const [balances, setBalances] = useState<AccountBalance[]>([]);
  const [loading, setLoading] = useState(false);
  const [adding, setAdding] = useState(false);
  const [msg, setMsg] = useState("");

  const reloadKeys = useCallback(async () => {
    try {
      const s = await api.getSettings();
      setKeys(s.provider_keys ?? []);
    } catch {
      /* ignore */
    }
  }, []);

  // 初始化 / 外部新增 Key 后重新读取
  useEffect(() => {
    (async () => {
      await reloadKeys();
      setLoaded(true);
    })();
  }, [reloadKeys, version]);

  const refreshBalances = useCallback(async () => {
    if (keys.length === 0) {
      setBalances([]);
      return;
    }
    setLoading(true);
    try {
      setBalances(await api.getBalances());
    } catch (e) {
      setMsg(`余额查询失败：${e}`);
    } finally {
      setLoading(false);
    }
  }, [keys.length]);

  // Key 数量或成员变化都要重新查询（删一个再加一个时数量不变）
  const keysKey = keys.map((k) => k.id).join(",");

  // 有 Key 时：立即查一次 + 每 300s 定时刷新 + 面板重新可见时刷新
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
  }, [keysKey, keys.length, refreshBalances]);

  async function removeKey(id: string) {
    try {
      const s = await api.getSettings();
      s.provider_keys = (s.provider_keys ?? []).filter((k) => k.id !== id);
      await api.saveSettings(s);
      setKeys(s.provider_keys);
      setMsg("已删除");
      onChanged?.();
    } catch (e) {
      setMsg(`删除失败：${e}`);
    }
  }

  if (!loaded || keys.length === 0) return null;

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-white/90">模型余额</span>
        <div className="flex items-center gap-3 text-[10px]">
          <button
            onClick={refreshBalances}
            disabled={loading}
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

      <div className="thin-scroll mt-2 max-h-[196px] space-y-1.5 overflow-y-auto pr-1">
        {balances.length === 0 && (
          <div className="text-[11px] text-white/40">查询中…</div>
        )}
        {balances.map((b) => {
          const primary = b.balances.find((x) => x.primary) ?? b.balances[0];
          const others = b.balances.filter((x) => x !== primary);
          return (
            <div key={b.id} className="rounded-lg bg-black/20 px-2 py-1.5">
              <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-1.5">
                  <StatusDot on={b.ok && b.is_available} />
                  <span className="shrink-0 rounded bg-white/10 px-1 py-[1px] text-[9px] text-white/55">
                    {providerName(b.provider)}
                  </span>
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
                <div className="mt-0.5 flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 pl-4">
                  {primary ? (
                    <span className="text-[13px] font-medium text-white/90">
                      {currencySymbol(primary.currency)}
                      {primary.amount}
                    </span>
                  ) : (
                    <span className="text-[11px] text-white/45">无余额信息</span>
                  )}
                  {others.map((x, i) => (
                    <span key={`${x.label}-${i}`} className="text-[10px] text-white/40">
                      {x.label} {currencySymbol(x.currency)}
                      {x.amount}
                    </span>
                  ))}
                  {!b.is_available && (
                    <span className="text-[10px] text-amber-300/80">余额不足</span>
                  )}
                </div>
              ) : (
                <div className="mt-0.5 pl-4 text-[10px] leading-snug text-rose-300/80">
                  {b.error}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {adding && (
        <div className="mt-2 border-t border-white/10 pt-2">
          <KeyForm
            onSaved={async () => {
              await reloadKeys();
              setAdding(false);
              setMsg("已添加，正在查询余额…");
              onChanged?.();
            }}
            onCancel={() => setAdding(false)}
          />
        </div>
      )}

      {msg && <div className="mt-1.5 text-[10px] text-white/40">{msg}</div>}
    </div>
  );
}
