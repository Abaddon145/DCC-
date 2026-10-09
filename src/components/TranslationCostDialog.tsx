import type { TranslationCostEstimate } from "../types";
import { useEffect, useRef, useState } from "react";

interface Props {
  estimate: TranslationCostEstimate | null; loading: boolean; error: string;
  onConfirm: () => Promise<void>; onCancel: () => void; onRetry: () => void;
  onRawSave?: () => Promise<void>;
}
export function combineTranslationCosts(parts: TranslationCostEstimate[]): TranslationCostEstimate {
  if (!parts.length) throw new Error("没有可预估的素材");
  const latest = parts[parts.length - 1];
  if (parts.some(part => part.month !== latest.month)) throw new Error("预估期间月份已变化，请重新预估");
  const result = { ...latest, rows: parts.flatMap(part => part.rows), warnings: [...new Set(parts.flatMap(part => part.warnings))] };
  for (const key of ["pendingFieldCount", "characters", "cacheHits", "tokensMin", "tokensMax", "onlineRequests", "baiduCharacters"] as const) result[key] = parts.reduce((sum, part) => sum + part[key], 0);
  result.wouldExceed = result.tokensMax > 0 && result.monthUsed + result.tokensMax > result.monthLimit;
  if (parts.length > 1) result.warnings.push("跨批次片段未假定缓存已预热，汇总预留可能偏高。月度已用取最新快照，不重复相加。");
  return result;
}
export function TranslationCostDialog({ estimate, loading, error, onConfirm, onCancel, onRetry, onRawSave }: Props) {
  const dialog = useRef<HTMLElement>(null), submitting = useRef(false);
  const [busy, setBusy] = useState(false), [actionError, setActionError] = useState("");
  useEffect(() => { const previous = document.activeElement as HTMLElement | null; dialog.current?.focus(); return () => previous?.focus(); }, []);
  const perform = async (action: () => Promise<void>) => {
    if (submitting.current) return;
    submitting.current = true; setBusy(true); setActionError("");
    try { await action(); } catch (failure) { setActionError(String(failure)); }
    finally { submitting.current = false; setBusy(false); }
  };
  const number = (value: number) => value.toLocaleString("zh-CN");
  return <div className="modal-backdrop translation-cost-backdrop"><section ref={dialog} tabIndex={-1} className="import-modal translation-cost-modal" role="dialog" aria-modal="true" aria-label="翻译费用确认" onKeyDown={event => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if (!submitting.current) onCancel(); }
    if (event.key === "Tab") {
      const buttons = [...(dialog.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") || [])];
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement) + (event.shiftKey ? -1 : 1);
      if (!buttons[index]) { event.preventDefault(); buttons[event.shiftKey ? buttons.length - 1 : 0]?.focus(); }
    }
  }}>
    <header className="modal-header"><div><span className="eyebrow">补齐缺失双语</span><h2>发送前核对</h2></div></header>
    <div className="import-body" aria-busy={loading || busy}>
      {loading && <p role="status">正在只读检查待译字段与本地缓存…</p>}
      {(error || actionError) && <p className="form-warning" role="alert">无法预估：{error || actionError}，未发送翻译请求。</p>}
      {!loading && !error && estimate && <>
        <div className="report-banner">待补译 {number(estimate.pendingFieldCount)} 个字段 · 待请求 {number(estimate.characters)} 字符 · 缓存命中 {number(estimate.cacheHits)} 段</div>
        <p>DeepSeek 保守预留区间：{number(estimate.tokensMin)}–{number(estimate.tokensMax)} Token</p>
        <p>{estimate.month} 已用 {number(estimate.monthUsed)} / 本机上限 {number(estimate.monthLimit)} Token（已含创作用量）</p>
        <p>{estimate.onlineRequests ? `预计 ${number(estimate.onlineRequests)} 次正常请求；百度待请求 ${number(estimate.baiduCharacters)} 字符` : "本次不会请求在线翻译；开关关闭、无待译字段或内容可在本地处理。"}</p>
        <small>这是预留区间，不是实际账单；发送时仍执行原有额度保护。</small>
        {estimate.wouldExceed && <p className="form-warning" role="alert">本次 DeepSeek 预留可能超过共享月度上限，已禁用在线补译。</p>}
        {estimate.warnings.map((warning, index) => <p className="section-help" key={index}>{warning}</p>)}
        {!!estimate.rows.length && <details><summary>逐项范围 · {estimate.rows.length} 项</summary><ul className="translation-cost-rows">{estimate.rows.map((row, index) => <li key={row.assetId || index}><strong>{row.name || row.assetId || "当前素材"}</strong><span>{row.pendingFields.join("、") || "无待译字段"} · {row.reason}</span></li>)}</ul></details>}
      </>}
    </div>
    <footer className="modal-footer">
      <button type="button" className="secondary-button" disabled={busy} onClick={onCancel}>取消</button>
      {error && <button type="button" className="secondary-button" disabled={loading || busy} onClick={onRetry}>重新预估</button>}
      {onRawSave && <button type="button" className="secondary-button" disabled={loading || busy} onClick={() => void perform(onRawSave)}>仅保存原文</button>}
      <button type="button" className="primary-button" disabled={loading || busy || !!error || !estimate || estimate.wouldExceed} onClick={() => void perform(onConfirm)}>确认并补译</button>
    </footer>
  </section></div>;
}
