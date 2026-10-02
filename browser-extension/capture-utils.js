export const IMAGE_LIMIT = 25 * 1024 * 1024;
export const VIDEO_LIMIT = 512 * 1024 * 1024;
export const CHUNK_SIZE = 128 * 1024;

export function publicUrl(raw) {
  const url = new URL(raw);
  const host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || (url.port && !['80', '443'].includes(url.port)) ||
      host === 'localhost' || /\.(localhost|local|internal|home|lan)$/.test(host) || host.includes(':') || /^\d+\.\d+\.\d+\.\d+$/.test(host)) {
    throw new Error('仅支持公开 HTTP/HTTPS 网页，不采集内网或带登录凭据的地址');
  }
  url.hash = '';
  return url.href;
}
export function sourceUrl(raw) {
  const url = new URL(publicUrl(raw));
  const sensitive = new Set(['token', 'accesstoken', 'refreshtoken', 'idtoken', 'authtoken', 'authorization', 'auth',
    'password', 'passwd', 'pwd', 'cookie', 'session', 'sessionid', 'sid', 'jwt', 'apikey', 'key', 'signature', 'sig',
    'credential', 'credentials', 'secret', 'clientsecret', 'code']);
  for (const key of [...url.searchParams.keys()]) {
    const normalized = key.toLowerCase().replace(/[-_]/g, '');
    if (sensitive.has(normalized) || /^x-(amz|goog)-/i.test(key)) url.searchParams.delete(key);
  }
  return url.href;
}
export function folderPath(folder, folders) {
  const names = [], seen = new Set(), byId = new Map(folders.filter(value => value.kind === folder.kind).map(value => [value.id, value]));
  for (let current = folder; current && !seen.has(current.id); current = byId.get(current.parentId)) {
    seen.add(current.id); names.unshift(current.name);
  }
  return names.join(' / ');
}
export function extensionFor(mime, kind) {
  const type = mime.toLowerCase().split(';')[0].trim();
  const formats = kind === 'video' ? { 'video/mp4': 'mp4', 'video/webm': 'webm' } :
    { 'image/jpeg': 'jpg', 'image/png': 'png', 'image/webp': 'webp', 'image/gif': 'gif' };
  return formats[type] || null;
}
export function base64(bytes) {
  let result = '';
  for (let i = 0; i < bytes.length; i += 8192) result += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(result);
}
export function resultText(result) {
  if (result.status === 'failed') return `处理失败：${result.message || '请在任务中心检查'}`;
  if (result.status !== 'completed') return '已暂存，等待栈藏处理；尚未确认保存成功';
  switch (result.result?.status) {
    case 'saved': return '文件已保存到栈藏';
    case 'duplicate': return '内容重复，已跳过；可在收件箱打开已有条目';
    case 'link': return '仅保存来源链接，未下载视频文件';
    default: return '处理结束，请在收件箱检查结果';
  }
}
