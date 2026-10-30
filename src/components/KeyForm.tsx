import { useState } from "react";
import { api } from "../ipc";
import { PROVIDERS } from "../providers";
import type { ProviderKey } from "../types";

interface Props {
  /// 保存成功回调（由调用方决定刷新余额 / 关闭表单）
  onSaved: () => void;
  onCancel?: () => void;
  cancelLabel?: string;
}

const inputCls =
  "w-full rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px] text-white/85 placeholder:text-white/30 focus:border-indigo-400/50 focus:outline-none";

/// 余额账号添加表单：选择平台 + 备注 + API Key（中转站额外填站点地址）。
/// 设置页（首个账号）与余额区（追加账号）共用同一套逻辑。
export function KeyForm({ onSaved, onCancel, cancelLabel = "取消" }: Props) {
  const [provider, setProvider] = useState("deepseek");
  const [label, setLabel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState("");

  const meta = PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0];

  async function save() {
    const key = apiKey.trim();
    if (!key) {
      setMsg("请填写 API Key");
      return;
    }
    if (meta.needsBaseUrl && !baseUrl.trim()) {
      setMsg("请填写站点地址");
      return;
    }
    setSaving(true);
    setMsg("");
    try {
      const s = await api.getSettings();
      const keys = s.provider_keys ?? [];
      const next: ProviderKey[] = [
        ...keys,
        {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          provider: meta.id,
          label: label.trim() || `${meta.name} ${keys.length + 1}`,
          api_key: key,
          base_url: meta.needsBaseUrl ? baseUrl.trim() : "",
        },
      ];
      s.provider_keys = next;
      await api.saveSettings(s);
      setLabel("");
      setApiKey("");
      setBaseUrl("");
      onSaved();
    } catch (e) {
      setMsg(`保存失败：${e}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-1.5">
      <select
        value={provider}
        onChange={(e) => setProvider(e.target.value)}
        className={inputCls + " [&>option]:bg-zinc-800"}
      >
        {PROVIDERS.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      <div className="text-[9px] leading-snug text-white/35">{meta.hint}</div>
      {meta.needsBaseUrl && (
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="站点地址，如 https://api.example.com"
          className={inputCls}
        />
      )}
      <input
        value={label}
        onChange={(e) => setLabel(e.target.value)}
        placeholder="备注（可选，如：主账号）"
        className={inputCls}
      />
      <input
        value={apiKey}
        onChange={(e) => setApiKey(e.target.value)}
        type="password"
        placeholder={meta.placeholder ?? "API Key"}
        className={inputCls}
      />
      <div className="flex items-center justify-between">
        <span className="text-[9px] text-white/30">仅保存在本机配置文件中</span>
        <div className="flex gap-2">
          {onCancel && (
            <button
              onClick={onCancel}
              className="text-[10px] text-white/45 transition hover:text-white/70"
            >
              {cancelLabel}
            </button>
          )}
          <button
            onClick={save}
            disabled={saving}
            className="rounded bg-indigo-500/20 px-2 py-0.5 text-[10px] text-indigo-100 transition hover:bg-indigo-500/30 disabled:opacity-40"
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>
      {msg && <div className="text-[10px] text-rose-300/80">{msg}</div>}
    </div>
  );
}
