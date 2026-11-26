// 国际化自检：字典 key 双向对齐、前端 t() 引用完整、官网 EN 字典与 HTML 标记对齐、内联 JS 语法。
// 用法：node scripts/check-i18n.mjs
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const problems = [];

function read(rel) {
  return fs.readFileSync(path.join(root, rel), "utf8");
}

function objectBlock(text, marker) {
  const at = text.search(marker);
  if (at < 0) return null;
  const open = text.indexOf("{", at);
  if (open < 0) return null;
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    if (text[i] === "}") {
      depth--;
      if (depth === 0) return text.slice(open + 1, i);
    }
  }
  return null;
}

function keysIn(block) {
  if (!block) return [];
  const keys = [];
  const re = /(?:^|[,{\s])(?:"([^"]+)"|'([^']+)')\s*:/g;
  let m;
  while ((m = re.exec(block))) keys.push(m[1] ?? m[2] ?? m[3]);
  return keys;
}

function diff(a, b) {
  const sa = new Set(a);
  const sb = new Set(b);
  return {
    onlyA: [...sa].filter((k) => !sb.has(k)),
    onlyB: [...sb].filter((k) => !sa.has(k)),
  };
}

// 1. 应用字典逐文件对齐
const dictFiles = ["app", "panels", "backend"];
const allZhKeys = new Set();
for (const name of dictFiles) {
  const src = read(`src/i18n/dict/${name}.ts`);
  const zh = keysIn(objectBlock(src, /export const zh\s*=/));
  const en = keysIn(objectBlock(src, /export const en\s*:/));
  const d = diff(zh, en);
  if (d.onlyA.length || d.onlyB.length) {
    problems.push(
      `dict/${name}.ts 键不一致：仅 zh=${d.onlyA.join(", ")}；仅 en=${d.onlyB.join(", ")}`
    );
  }
  zh.forEach((k) => allZhKeys.add(k));

  const enBlock = objectBlock(src, /export const en\s*:/);
  const cjkInEn = [...(enBlock ?? "").matchAll(/:\s*"((?:[^"\\]|\\.)*)"/g)]
    .map((m) => m[1])
    .filter((value) => /[\u4e00-\u9fff]/.test(value));
  if (cjkInEn.length) {
    problems.push(`dict/${name}.ts 英文译文仍有中文：${cjkInEn.slice(0, 5).join(" | ")}`);
  }
}

// 2. 前端 t() 引用必须有字典项（排除字典自身与框架文件）
const sourceFiles = [];
function walk(dir) {
  for (const entry of fs.readdirSync(path.join(root, dir), { withFileTypes: true })) {
    const rel = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(rel);
    else if (/\.(ts|tsx)$/.test(entry.name) && !rel.includes("i18n/dict/") && rel !== "src/i18n/index.ts") {
      sourceFiles.push(rel);
    }
  }
}
walk("src");
const used = new Set();
for (const rel of sourceFiles) {
  const src = read(rel);
  for (const m of src.matchAll(/\bt\(\s*"((?:[^"\\]|\\.)*)"/g)) used.add(m[1]);
}
const missing = [...used].filter((k) => !allZhKeys.has(k));
if (missing.length) problems.push(`t() 引用了未定义 key：${missing.join(", ")}`);

// 3. backend patterns 的 key 必须在 backend 字典中
const backendSrc = read("src/i18n/dict/backend.ts");
const patternKeys = [...backendSrc.matchAll(/\bkey\s*:\s*"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);
const backendZh = new Set(keysIn(objectBlock(backendSrc, /export const zh\s*=/)));
const badPatterns = patternKeys.filter((k) => !backendZh.has(k));
if (badPatterns.length) problems.push(`backend patterns 引用了未定义 key：${badPatterns.join(", ")}`);

// 4. 官网 i18n 标记与 EN 字典对齐
const site = read("website/index.html");
const htmlKeys = [];
for (const m of site.matchAll(/data-i18n(?:-html|-attr)?\s*=\s*"([^"]+)"/g)) {
  if (m[0].startsWith("data-i18n-attr")) {
    // 形如 "alt:key1;title:key2"，字典 key 是冒号后的部分
    m[1].split(";").forEach((pair) => {
      const i = pair.indexOf(":");
      if (i >= 0) htmlKeys.push(pair.slice(i + 1).trim());
    });
  } else {
    htmlKeys.push(m[1]);
  }
}
const enBlock = objectBlock(site, /const EN\s*=\s*\{/);
const siteEnKeys = keysIn(enBlock);
const siteDiff = diff(new Set(htmlKeys), new Set(siteEnKeys));
if (siteDiff.onlyA.length || siteDiff.onlyB.length) {
  problems.push(
    `官网 i18n key 不一致：仅 HTML=${siteDiff.onlyA.join(", ")}；仅 EN=${siteDiff.onlyB.join(", ")}`
  );
}

// 5. 官网内联 JS 语法检查（排除 JSON-LD）
const scripts = [...site.matchAll(/<script(?![^>]*application\/ld\+json)[^>]*>([\s\S]*?)<\/script>/g)]
  .map((m) => m[1])
  .join("\n;\n");
const tmp = path.join(os.tmpdir(), `switchkit-site-${process.pid}.js`);
fs.writeFileSync(tmp, scripts);
try {
  execFileSync(process.execPath, ["--check", tmp], { stdio: "pipe" });
} catch (e) {
  problems.push(`官网内联 JS 语法错误：${String(e.stderr || e).slice(0, 400)}`);
} finally {
  fs.unlinkSync(tmp);
}

if (problems.length) {
  console.error("i18n 自检失败：");
  problems.forEach((p) => console.error(" - " + p));
  process.exit(1);
}
console.log(
  `i18n 自检通过：应用字典 ${allZhKeys.size} 项，t() 引用 ${used.size} 项，官网 key ${siteEnKeys.length} 项`
);
