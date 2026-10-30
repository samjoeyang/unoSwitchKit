import { useState } from "react";
import { api } from "../ipc";
import type { DeepSeekKey } from "../types";

interface Props {
  /// 保存成功回调（由调用方决定刷新余额 / 关闭表单）
  onSaved: () => void;
  onCancel?: () => void;
  cancelLabel?: string;
}

/// DeepSeek API Key 添加表单：读取现有 Key 后追加并写入本机配置。
/// 设置页（首个 Key）与余额区（追加 Key）共用同一套逻辑。
export function DeepSeekKeyForm({ onSaved, onCancel, cancelLabel = "取消" }: Props) {
  const [label, setLabel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState("");

  async function save() {
    const key = apiKey.trim();
    if (!key) {
      setMsg("请填写 API Key");
      return;
    }
    setSaving(true);
    setMsg("");
    try {
      const s = await api.getSettings();
      const keys = s.deepseek_keys ?? [];
      const next: DeepSeekKey[] = [
        ...keys,
        {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          label: label.trim() || `Key ${keys.length + 1}`,
          api_key: key,
        },
      ];
      s.deepseek_keys = next;
      await api.saveSettings(s);
      setLabel("");
      setApiKey("");
      onSaved();
    } catch (e) {
      setMsg(`保存失败：${e}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-1.5">
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
