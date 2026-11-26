import { useEffect, useMemo, useState } from "react";
import QRCode from "qrcode";
import { Download, QrCode, CircleAlert, Check } from "lucide-react";
import { api } from "../ipc";
import { useT, tb } from "../i18n";

type T = ReturnType<typeof useT>;

/**
 * 二维码静默区（quiet zone）：ISO/IEC 18004 要求码符号四周至少 4 个模块宽的空白。
 * 没有静默区时扫码器无法定位码边界，会出现「黑到边扫不出来」。此处默认取 4，
 * 并允许用户在 4–8 之间调整；生成的 PNG 同样包含该边距。
 */
const DEFAULT_MARGIN = 4;
const MIN_MARGIN = 4;
const MAX_MARGIN = 8;

/** 用户输入的链接补全协议：无 scheme 时默认 https://。 */
function normalizeUrl(raw: string): string {
  const v = raw.trim();
  if (!v) return "";
  // 已有协议（http/https/其他）时不改动
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(v)) return v;
  return `https://${v}`;
}

/** 做基本合法性校验：必须能解析成 http(s) URL。返回错误文案，合法时返回 null。 */
function validateUrl(v: string, t: T): string | null {
  if (!v) return null;
  try {
    const u = new URL(v);
    if (u.protocol !== "http:" && u.protocol !== "https:") {
      return t("仅支持 http / https 链接");
    }
    if (!u.hostname) return t("链接缺少域名");
    return null;
  } catch {
    return t("链接格式不正确，请检查后重试");
  }
}

export function QrCodeSection() {
  const t = useT();
  const [input, setInput] = useState("");
  const [margin, setMargin] = useState(DEFAULT_MARGIN);
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);

  const normalized = useMemo(() => normalizeUrl(input), [input]);
  const urlError = useMemo(() => validateUrl(normalized, t), [normalized, t]);

  // 输入变化即重新生成（含静默区），空输入清空。qrcode 生成为纯前端，不触发任何权限。
  useEffect(() => {
    let cancelled = false;
    if (!normalized || urlError) {
      setDataUrl(null);
      return;
    }
    QRCode.toDataURL(normalized, {
      errorCorrectionLevel: "M",
      margin,
      width: 512,
      color: { dark: "#000000ff", light: "#ffffffff" },
    })
      .then((url) => {
        if (!cancelled) {
          setDataUrl(url);
          setError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setDataUrl(null);
          setError(t("生成失败：{detail}", { detail: tb(e) }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [normalized, urlError, margin, t]);

  async function savePng() {
    if (!dataUrl) return;
    setSaving(true);
    setSaveMsg(null);
    try {
      const path = await api.saveQrcodePng(dataUrl);
      setSaveMsg(t("已保存到 {path}", { path }));
    } catch (e) {
      setSaveMsg(t("保存失败：{detail}", { detail: tb(e) }));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mt-3 rounded-xl border border-white/10 bg-white/5 p-3">
      <div className="flex items-center gap-1.5">
        <QrCode size={14} className="text-indigo-300/90" />
        <span className="text-[13px] font-medium text-white/90">{t("生成二维码")}</span>
      </div>
      <p className="mt-1 text-[10px] leading-snug text-white/35">
        {t("输入链接生成可扫描的二维码，四周保留白边（静默区）")}
      </p>

      <input
        value={input}
        onChange={(e) => {
          setInput(e.target.value);
          setSaveMsg(null);
        }}
        placeholder={t("example.com 或 https://example.com/path")}
        spellCheck={false}
        autoCapitalize="off"
        className="mt-2 w-full rounded border border-white/10 bg-black/20 px-2 py-1.5 text-[11px] text-white/85 placeholder:text-white/30 focus:border-indigo-400/50 focus:outline-none"
      />

      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="shrink-0 text-[10px] text-white/45">{t("白边宽度")}</span>
        <div className="flex flex-1 items-center gap-2">
          <input
            type="range"
            min={MIN_MARGIN}
            max={MAX_MARGIN}
            step={1}
            value={margin}
            onChange={(e) => {
              setMargin(Number(e.target.value));
              setSaveMsg(null);
            }}
            className="h-1 flex-1 cursor-pointer accent-indigo-400"
            aria-label={t("二维码白边宽度")}
          />
          <span className="w-8 shrink-0 text-right font-mono text-[10px] text-white/55">
            {margin}
          </span>
        </div>
      </div>

      {(urlError || error) && (
        <div className="mt-2 flex items-start gap-1 text-[10px] leading-snug text-rose-300/85">
          <CircleAlert size={12} className="mt-[1px] shrink-0" />
          <span>{urlError || error}</span>
        </div>
      )}

      {dataUrl && (
        <div className="mt-2.5 flex flex-col items-center">
          {/* 内边距叠加在二维码自带静默区之外，确保视觉上黑码不贴边 */}
          <div className="rounded-lg bg-white p-2.5 shadow-inner">
            <img src={dataUrl} alt={t("生成的二维码")} className="h-[150px] w-[150px]" />
          </div>
          <div className="mt-2 text-[10px] break-all text-center text-white/40" title={normalized}>
            {normalized}
          </div>
          <button
            onClick={() => void savePng()}
            disabled={saving}
            className="mt-2 flex items-center gap-1.5 rounded-md bg-indigo-500/20 px-3 py-1 text-[11px] text-indigo-100 transition hover:bg-indigo-500/35 disabled:opacity-40"
          >
            <Download size={12} />
            {saving ? t("保存中…") : t("保存图片")}
          </button>
          {saveMsg && (
            <div className="mt-1.5 flex items-center gap-1 text-center text-[10px] leading-snug text-white/45">
              {saveMsg.startsWith(t("已保存")) && <Check size={11} className="shrink-0 text-emerald-300/80" />}
              <span className="break-all">{saveMsg}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
