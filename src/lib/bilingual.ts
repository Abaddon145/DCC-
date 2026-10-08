import { useEffect, useRef, useState } from "react";
import type { AssetDetail, ContentLanguage, TranslationSettings } from "../types";
import { api } from "./api";

export function missingTranslationFields(localizations: AssetDetail["localizations"]): string[] {
  const missing: string[] = [];
  for (const [source, target, label] of [["en", "zh-CN", "中文"], ["zh-CN", "en", "英文"]] as const) {
    const from = localizations[source], to = localizations[target];
    for (const [key, name] of [["name", "名称"], ["description", "描述"], ["tags", "标签"], ["license", "许可"]] as const) {
      const has = (locale: ContentLanguage) => key === "tags" ? !!localizations[locale]?.tags.length : !!localizations[locale]?.[key].trim();
      if (from && has(source) && (!to || !has(target))) missing.push(`${label}${name}`);
    }
  }
  return missing;
}

/** Both importers show and persist the same preference; failures remain visible. */
export function useFabTranslationSettings() {
  const [settings, setSettings] = useState<TranslationSettings | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const changing = useRef(false);
  useEffect(() => {
    let active = true; mounted.current = true;
    void api.getTranslationSettings().then(value => { if (active) setSettings(value); })
      .catch(reason => { if (active) setError(`读取翻译设置失败：${String(reason)}`); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; mounted.current = false; };
  }, []);
  const toggle = async (enabled: boolean) => {
    if (changing.current) return;
    changing.current = true; setBusy(true); setError("");
    try {
      await api.setFabAutoTranslate(enabled);
      if (mounted.current) setSettings(previous => previous && ({ ...previous, fabAutoTranslate: enabled }));
    } catch (reason) { if (mounted.current) setError(`保存翻译设置失败：${String(reason)}`); }
    finally { changing.current = false; if (mounted.current) setBusy(false); }
  };
  return { settings, busy, error, toggle };
}
