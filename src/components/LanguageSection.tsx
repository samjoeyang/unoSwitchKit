import { setLang, useLang, useT, type Lang } from "../i18n";

/** 设置页语言切换：与官网共用 localStorage 键 sk-lang，切换后立即重渲染。 */
export function LanguageSection() {
  const lang = useLang();
  const t = useT();

  const options: Array<{ value: Lang; label: string }> = [
    { value: "zh", label: "中文" },
    { value: "en", label: "English" },
  ];

  return (
    <div className="py-2">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-[12px] text-white/85">{t("界面语言")}</div>
          <div className="text-[10px] text-white/35">
            {t("切换后界面立即更新，偏好保存在本机")}
          </div>
        </div>
        <div className="flex gap-1">
          {options.map((o) => (
            <button
              key={o.value}
              onClick={() => setLang(o.value)}
              aria-pressed={lang === o.value}
              className={
                "rounded px-2 py-1 text-[10px] transition " +
                (lang === o.value
                  ? "bg-white/15 text-white/90"
                  : "bg-white/5 text-white/45 hover:text-white/75")
              }
            >
              {o.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
