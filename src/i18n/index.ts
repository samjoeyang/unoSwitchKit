import { useCallback, useSyncExternalStore } from "react";
import { zh as zhApp, en as enApp } from "./dict/app";
import { zh as zhPanels, en as enPanels } from "./dict/panels";
import { zh as zhBackend, en as enBackend, patterns } from "./dict/backend";

/** 所有语言字典共用同一组 key（中文原文），任缺一项都会在编译期报错。 */
const ZH = { ...zhApp, ...zhPanels, ...zhBackend };
const EN: Record<keyof typeof ZH, string> = {
  ...enApp,
  ...enPanels,
  ...enBackend,
};

export type Lang = "zh" | "en";
export type TranslationKey = Extract<keyof typeof ZH, string>;
export type TranslateVars = Record<string, string | number>;

const STORAGE_KEY = "sk-lang";

function detectInitialLang(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "zh" || saved === "en") return saved;
  } catch {
    /* WebView 禁用存储时回退系统语言 */
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

let currentLang: Lang = detectInitialLang();
const listeners = new Set<() => void>();

function applyLang(lang: Lang) {
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
}

// 模块加载时同步设置一次，避免首帧 html lang 仍是模板值。
if (typeof document !== "undefined") applyLang(currentLang);

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot() {
  return currentLang;
}

export function getLang(): Lang {
  return currentLang;
}

export function setLang(lang: Lang) {
  if (lang === currentLang) return;
  currentLang = lang;
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    /* 忽略存储失败 */
  }
  applyLang(lang);
  listeners.forEach((l) => l());
}

export function useLang(): Lang {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

function interpolate(text: string, vars?: TranslateVars): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match
  );
}

export function translate(
  lang: Lang,
  key: TranslationKey,
  vars?: TranslateVars
): string {
  const source = lang === "en" ? EN[key] : ZH[key];
  return interpolate(source ?? key, vars);
}

/** 无组件上下文（事件回调、模块函数）中使用的即时翻译。 */
export function t(key: TranslationKey, vars?: TranslateVars): string {
  return translate(currentLang, key, vars);
}

/** 组件内使用：语言切换后自动重渲染。 */
export function useT() {
  const lang = useLang();
  return useCallback(
    (key: TranslationKey, vars?: TranslateVars) => translate(lang, key, vars),
    [lang]
  );
}

function rawMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

/**
 * 翻译 Rust IPC 透传的文案：Rust 侧保持中文原文作为稳定 key，
 * 避免改动两百余处后端字面量与回归测试；前端按当前语言渲染。
 * 动态错误用 patterns 表捕获参数后再翻译，未收录内容原样返回。
 */
export function tb(error: unknown): string {
  const raw = rawMessage(error);
  if (Object.prototype.hasOwnProperty.call(ZH, raw)) {
    return translate(currentLang, raw as TranslationKey);
  }
  for (const p of patterns) {
    const m = raw.match(p.re);
    if (!m) continue;
    const vars: TranslateVars = {};
    p.params?.forEach((name, i) => {
      vars[name] = m[i + 1] ?? "";
    });
    return translate(currentLang, p.key as TranslationKey, vars);
  }
  return raw;
}

/** 登录门控文案识别：中英两侧都要能认出来。 */
export function isAuthError(error: unknown): boolean {
  const raw = rawMessage(error);
  return (
    raw.includes("需要登录") ||
    raw.includes("登录状态已失效") ||
    raw.includes("sign in") ||
    raw.includes("Sign in")
  );
}

export { ZH, EN };
