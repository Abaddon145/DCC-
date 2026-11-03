import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { TranslationPreferences, TranslationSettings } from "../types";

const defaults: TranslationPreferences = { providerOrder: ["deepseek", "baidu"], autoFallback: true, cacheEnabled: true, deepseekMonthlyLimit: 1_000_000 };

export function TranslationServiceOptions({ settings, reload, notify }: { settings: TranslationSettings | null; reload: () => Promise<unknown>; notify: (message: string, error?: boolean) => void }) {
  const [key, setKey] = useState("");
  const [preferences, setPreferences] = useState(defaults);
  const [busy, setBusy] = useState(false);
  useEffect(() => { setPreferences(settings?.preferences ?? defaults); }, [settings?.preferences]);
  const run = async (operation: () => Promise<void>) => {
    setBusy(true);
    try { await operation(); await reload(); }
    catch (error) { notify(String(error), true); }
    finally { setBusy(false); }
  };
  const saveKey = async () => { await api.saveDeepseekCredentials(key); setKey(""); notify("DeepSeek 凭据已保存到 Windows 凭据管理器"); };
  const test = async () => {
    if (key.trim()) await saveKey();
    const result = await api.testTranslationService("deepseek");
    notify(result.message);
  };
  const usage = settings?.usage;
  return <section className="translation-provider-options">
    <h3>视频分析设置</h3>
    <p>视频分析与翻译、创作共享同一个 DeepSeek 凭据和本机月度额度。分析前会展示选中的字幕、采样帧与额度，确认后才发送；不会上传整段视频。</p>
    <p>适用于 CG、UE 和 DCC 内容，继续使用专业术语库。API 按 Token 计费；翻译仅发送待翻译文字，AI 创作仅发送确认过的文字上下文，不上传媒体文件。</p>
    <label className="translation-key-field"><span>DeepSeek API Key</span><input aria-label="DeepSeek API Key" type="password" autoComplete="new-password" value={key} onChange={event => setKey(event.target.value)} placeholder={settings?.deepseekConfigured ? "已配置，输入新密钥可替换" : "填写自己的 DeepSeek API Key"} /></label>
    <div className="translation-settings-actions">
      <button className="primary-button" disabled={busy || !key.trim()} onClick={() => void run(saveKey)}>保存 DeepSeek 凭据</button>
      <button className="secondary-button" disabled={busy || (!key.trim() && !settings?.deepseekConfigured)} onClick={() => void run(test)}>{key.trim() ? "保存并测试 DeepSeek" : "测试 DeepSeek"}</button>
      <button className="secondary-button danger" disabled={busy || !settings?.deepseekConfigured} onClick={() => { if (window.confirm("删除本机 DeepSeek 凭据？")) void run(async () => { await api.deleteDeepseekCredentials(); notify("DeepSeek 凭据已删除"); }); }}>删除 DeepSeek 凭据</button>
    </div>
    <h3>翻译顺序与共享用量</h3>
    <label className="setting-row"><span>服务优先级</span><select aria-label="翻译服务优先级" value={preferences.providerOrder[0]} onChange={event => setPreferences(value => ({ ...value, providerOrder: event.target.value === "deepseek" ? ["deepseek", "baidu"] : ["baidu", "deepseek"] }))}><option value="deepseek">DeepSeek → 百度</option><option value="baidu">百度 → DeepSeek</option></select></label>
    <label className="setting-row check"><span>额度不足、限流或临时故障时使用备用服务</span><input type="checkbox" checked={preferences.autoFallback} onChange={event => setPreferences(value => ({ ...value, autoFallback: event.target.checked }))} /></label>
    <label className="setting-row check"><span>复用本地成功译文缓存</span><input type="checkbox" checked={preferences.cacheEnabled} onChange={event => setPreferences(value => ({ ...value, cacheEnabled: event.target.checked }))} /></label>
    <label className="setting-row"><span>DeepSeek 本机月度 Token 上限</span><input aria-label="DeepSeek 本机月度 Token 上限" type="number" min={1} max={1_000_000_000} step={10000} value={preferences.deepseekMonthlyLimit} onChange={event => setPreferences(value => ({ ...value, deepseekMonthlyLimit: Number(event.target.value) }))} /></label>
    <p className="translation-usage">{usage?.month ?? "本月"} · DeepSeek {usage?.deepseekTokens.toLocaleString("zh-CN") ?? "0"} Token · 百度发送文字 {usage?.baiduCharacters.toLocaleString("zh-CN") ?? "0"} 字符<br />创作 {usage?.creativeTokens?.toLocaleString("zh-CN") ?? "0"} Token · 课程分析 {usage?.courseTokens?.toLocaleString("zh-CN") ?? "0"} Token · 翻译 {Math.max(0,(usage?.deepseekTokens??0)-(usage?.creativeTokens??0)-(usage?.courseTokens??0)).toLocaleString("zh-CN")} Token<br />缓存 {usage?.cacheEntries ?? 0} 段 · 本月复用 {usage?.cacheHits ?? 0} 次</p>
    <p>这是本软件的本机记录，并非服务商账户余额。Token 包含请求前预留值，超时或异常退出后会保守计入；百度字符数不包含服务内部重试。达到本机上限后暂停新请求，不切换服务绕过上限。每月自动重新计数。</p>
    <div className="translation-settings-actions">
      <button className="primary-button" disabled={busy || !Number.isSafeInteger(preferences.deepseekMonthlyLimit) || preferences.deepseekMonthlyLimit < 1 || preferences.deepseekMonthlyLimit > 1_000_000_000} onClick={() => void run(async () => { await api.saveTranslationPreferences(preferences); notify("翻译顺序与用量设置已保存"); })}>保存翻译设置</button>
      <button className="secondary-button" disabled={busy} onClick={() => void run(async () => { notify("翻译用量已刷新"); })}>刷新用量</button>
      <button className="secondary-button" disabled={busy || !usage?.cacheEntries} onClick={() => { if (window.confirm("清空本机译文缓存？已保存到素材中的译文不会删除，月度用量不会重置。")) void run(async () => { await api.clearTranslationCache(); notify("本机译文缓存已清空"); }); }}>清空译文缓存</button>
    </div>
  </section>;
}
