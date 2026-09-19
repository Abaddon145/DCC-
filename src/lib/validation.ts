import type { AssetInput } from "../types";

export interface ValidationResult { errors: Record<string, string>; warnings: string[] }

export function validateAsset(input: AssetInput): ValidationResult {
  const errors: Record<string, string> = {};
  const warnings: string[] = [];
  const names = Object.values(input.localizations || {}).map(value => value?.name.trim() || "");
  if (!names.some(Boolean)) errors.name = "请至少填写一种语言的素材名称";
  if (names.some(name => name.length > 200)) errors.name = "名称不能超过 200 个字符";
  if (input.shareUrl.trim()) {
    try {
      const url = new URL(input.shareUrl);
      if (!['http:', 'https:'].includes(url.protocol)) errors.shareUrl = "仅支持 http/https 链接";
      if (!/(^|\.)pan\.baidu\.com$/i.test(url.hostname)) warnings.push("这不是百度网盘域名，保存前请确认链接来源");
    } catch {
      errors.shareUrl = "请输入有效的分享链接";
    }
  }
  if (input.sourceUrl.trim()) {
    try {
      const url = new URL(input.sourceUrl);
      if (!['http:', 'https:'].includes(url.protocol)) errors.sourceUrl = "来源仅支持 http/https 链接";
    } catch { errors.sourceUrl = "请输入有效的来源地址"; }
  }
  if (input.sizeBytes !== null && input.sizeBytes < 0) errors.sizeBytes = "素材大小不能为负数";
  return { errors, warnings };
}

export function splitValues(value: string): string[] {
  const seen = new Set<string>();
  return value.split(/[;,；，]/).map(v => v.trim()).filter(value => value && !seen.has(value.toLocaleLowerCase()) && !!seen.add(value.toLocaleLowerCase()));
}

export function formatBytes(value: number | null): string {
  if (value === null) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let amount = value;
  let index = 0;
  while (amount >= 1024 && index < units.length - 1) { amount /= 1024; index += 1; }
  return `${amount >= 10 || index === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[index]}`;
}

export function parseSize(value: string): number | null {
  if (!value.trim()) return null;
  const match = value.trim().match(/^(\d+(?:\.\d+)?)\s*(b|kb|mb|gb|tb)?$/i);
  if (!match) return Number.NaN;
  const power = ["b", "kb", "mb", "gb", "tb"].indexOf((match[2] || "b").toLowerCase());
  return Math.round(Number(match[1]) * 1024 ** power);
}
