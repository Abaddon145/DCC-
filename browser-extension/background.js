import { publicUrl, sourceUrl, extensionFor, base64, resultText, IMAGE_LIMIT, VIDEO_LIMIT, CHUNK_SIZE } from './capture-utils.js';

const HOST = 'com.stackdcc.capture';
let port;
let nextRequest = 0;
const waiting = new Map();
let activeJob = null;

function native(action, payload = {}) {
  if (!port) {
    port = chrome.runtime.connectNative(HOST);
    port.onMessage.addListener(message => {
      const pending = waiting.get(message.requestId);
      if (!pending) return;
      waiting.delete(message.requestId); clearTimeout(pending.timeout);
      message.ok ? pending.resolve(message.value) : pending.reject(new Error(message.error || '栈藏处理失败'));
    });
    port.onDisconnect.addListener(() => {
      const error = chrome.runtime.lastError?.message || '栈藏连接中断'; port = null;
      for (const pending of waiting.values()) { clearTimeout(pending.timeout); pending.reject(new Error(`${error}。请在栈藏收件箱连接扩展 ID。`)); }
      waiting.clear();
    });
  }
  const requestId = ++nextRequest;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { waiting.delete(requestId); reject(new Error('通信超时，未确认保存成功')); }, 30000);
    waiting.set(requestId, { resolve, reject, timeout });
    try { port.postMessage({ requestId, action, ...payload }); }
    catch (error) { clearTimeout(timeout); waiting.delete(requestId); reject(error); }
  });
}
async function progress(value) {
  await chrome.storage.session.set({ progress: value });
  await chrome.action.setBadgeText({ text: value.busy ? '…' : value.error ? '!' : '' });
  await chrome.action.setTitle({ title: value.message || '保存至栈藏' });
}
async function panel() {
  try { await chrome.action.openPopup(); }
  catch { await chrome.windows.create({ url: chrome.runtime.getURL('popup.html'), type: 'popup', width: 460, height: 650 }); }
}
function makeCapture(item, libraryId, options, extension) {
  return { id: crypto.randomUUID(), libraryId, title: (item.title || '网页参考').slice(0, 240),
    pageUrl: sourceUrl(item.pageUrl), mediaUrl: '',
    kind: item.kind, extension, folderId: item.kind === 'link' ? null : options.folderId || null,
    tags: (options.tags || []).slice(0, 50), projectId: options.projectId || null, createdAt: new Date().toISOString() };
}
async function transfer(item, libraryId, options, parentSignal) {
  const controller = new AbortController(), signal = controller.signal;
  const cancel = () => controller.abort(new Error('采集已取消'));
  parentSignal.addEventListener('abort', cancel, { once: true });
  if (parentSignal.aborted) cancel();
  const deadline = setTimeout(() => controller.abort(new Error('资源读取超时，请重试或使用区域截图')), item.kind === 'video' ? 180000 : 60000);
  let response;
  let extension = null;
  let captureItem = { ...item };
  let uploadStarted = false, finished = false;
  try {
  signal.throwIfAborted();
  if (item.kind !== 'link') {
    try {
      // Never export cookies or request headers; the browser sends them only to the visited origin.
      const url = item.dataUrl || publicUrl(item.mediaUrl);
      response = await fetch(url, { credentials: item.kind === 'video' ? 'omit' : 'include', signal, redirect: 'error' });
      if (!response.ok || !response.body) throw new Error(`资源读取失败 (${response.status})`);
      extension = extensionFor(response.headers.get('content-type') || '', item.kind);
      if (!extension) throw new Error('不支持的格式或流媒体');
      const limit = item.kind === 'video' ? VIDEO_LIMIT : IMAGE_LIMIT;
      if (Number(response.headers.get('content-length')) > limit) throw new Error('文件超出采集大小限制');
    } catch (error) {
      signal.throwIfAborted();
      if (item.kind !== 'video') throw error;
      await response?.body?.cancel().catch(() => {});
      captureItem.kind = 'link'; response = null; extension = null;
    }
  }
  if (captureItem.kind === 'link' && item.posterUrl?.startsWith('http')) {
    try {
      const poster = await fetch(publicUrl(item.posterUrl), { credentials: 'omit', signal, redirect: 'error' });
      const type = extensionFor(poster.headers.get('content-type') || '', 'image');
      if (poster.ok && poster.body && type && Number(poster.headers.get('content-length')) <= IMAGE_LIMIT) { response = poster; extension = type; }
      else await poster.body?.cancel().catch(() => {});
    } catch { signal.throwIfAborted(); /* A cover is optional; cancellation is not. */ }
  }
  signal.throwIfAborted();
  const capture = makeCapture(captureItem, libraryId, options, extension);
  uploadStarted = true;
  await native('begin', { capture });
  signal.throwIfAborted();
  let total = 0;
    if (response) {
      const reader = response.body.getReader();
      try {
        for (;;) {
          signal.throwIfAborted();
          const { value, done } = await reader.read(); if (done) break;
          total += value.length;
          if (total > (captureItem.kind === 'video' ? VIDEO_LIMIT : IMAGE_LIMIT)) throw new Error('文件超出采集大小限制');
          for (let i = 0; i < value.length; i += CHUNK_SIZE) {
            await native('chunk', { data: base64(value.subarray(i, i + CHUNK_SIZE)) });
            signal.throwIfAborted();
          }
        }
      } finally { await reader.cancel().catch(() => {}); }
    }
    signal.throwIfAborted();
    await native('finish', { bytes: total }); finished = true;
    clearTimeout(deadline);
  // Long processing stays in the app task center. This timeout is never called success.
  for (let i = 0; i < 45; i++) {
    if (parentSignal.aborted) return { id: capture.id, status: 'queued', message: '文件已暂存；应用任务可在任务中心取消' };
    const result = await native('result', { id: capture.id });
    if (result.status !== 'queued') return { id: capture.id, ...result };
    await new Promise(resolve => setTimeout(resolve, 1000));
  }
  return { id: capture.id, status: 'queued' };
  } catch (error) {
    if (uploadStarted && !finished) await native('cancel').catch(() => {});
    signal.throwIfAborted(); throw error;
  } finally {
    clearTimeout(deadline); parentSignal.removeEventListener('abort', cancel);
    await response?.body?.cancel().catch(() => {});
  }
}
async function capture(items, options = {}) {
  if (activeJob) throw new Error('已有采集正在进行，请等待或取消');
  if (!Array.isArray(items) || !items.length || items.length > 100) throw new Error('单次采集 1–100 项');
  activeJob = new AbortController();
  const results = [];
  try {
  const hello = await native('hello');
  const confirmed = (await chrome.storage.local.get('confirmedLibrary')).confirmedLibrary;
  if (hello.started || !confirmed || confirmed !== hello.connection.libraryId || !hello.connection.enabled) {
    await chrome.storage.session.set({ pendingItems: items }); await panel();
    throw new Error('请在采集面板确认当前目标素材库');
  }
    for (const [index, item] of items.entries()) {
      if (activeJob.signal.aborted) break;
      await progress({ busy: true, current: index + 1, total: items.length, message: `采集中 ${index + 1}/${items.length} · ${item.title || '网页参考'}` });
      try {
        const result = await transfer(item, hello.connection.libraryId, options, activeJob.signal);
        results.push({ title: item.title, ...result, message: resultText(result) });
      } catch (error) { results.push({ title: item.title, status: 'failed', message: String(error.message || error) }); }
    }
    const failed = results.filter(r => r.status === 'failed').length;
    await chrome.storage.session.set({ results, pendingItems: [] });
    await progress({ busy: false, error: failed > 0, message: `已处理 ${results.length}/${items.length} 项${failed ? `，失败 ${failed} 项` : ''}；查看面板获取逐项结果` });
    return results;
  } finally { activeJob = null; }
}

async function screenshot(tabId) {
  const tab = await chrome.tabs.get(tabId); publicUrl(tab.url);
  const [selected] = await chrome.scripting.executeScript({ target: { tabId }, func: () => new Promise(resolve => {
    const overlay = document.createElement('div'), box = document.createElement('div');
    overlay.style.cssText = 'position:fixed;inset:0;z-index:2147483647;cursor:crosshair;background:#0003';
    box.style.cssText = 'position:absolute;border:2px solid #dda349;background:#fff1;pointer-events:none';
    overlay.append(box); document.documentElement.append(overlay); let start;
    const cleanup = () => { overlay.remove(); window.removeEventListener('keydown', key, true); clearTimeout(timeout); };
    const key = event => { if (event.key === 'Escape') { event.preventDefault(); cleanup(); resolve(null); } };
    const timeout = setTimeout(() => { cleanup(); resolve(null); }, 90000);
    window.addEventListener('keydown', key, true);
    overlay.onpointerdown = event => { start = { x: event.clientX, y: event.clientY }; overlay.setPointerCapture(event.pointerId); };
    overlay.onpointermove = event => { if (!start) return; Object.assign(box.style, { left: `${Math.min(start.x, event.clientX)}px`, top: `${Math.min(start.y, event.clientY)}px`, width: `${Math.abs(start.x - event.clientX)}px`, height: `${Math.abs(start.y - event.clientY)}px` }); };
    overlay.onpointerup = event => {
      if (!start) return;
      const rectangle = { x: Math.min(start.x, event.clientX), y: Math.min(start.y, event.clientY), width: Math.abs(start.x - event.clientX), height: Math.abs(start.y - event.clientY), viewportWidth: innerWidth, viewportHeight: innerHeight };
      cleanup(); setTimeout(() => resolve(rectangle.width > 2 && rectangle.height > 2 ? rectangle : null), 150);
    };
  }) });
  const rectangle = selected.result;
  if (!rectangle) { await progress({ busy: false, message: '截图已取消' }); return; }
  const active = await chrome.tabs.query({ active: true, windowId: tab.windowId });
  if (active[0]?.id !== tabId || active[0].url !== tab.url) throw new Error('当前网页已切换，请重新截图');
  const shot = await chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' });
  const after = await chrome.tabs.query({ active: true, windowId: tab.windowId });
  if (after[0]?.id !== tabId || after[0].url !== tab.url) throw new Error('当前网页已切换，请重新截图');
  const image = await createImageBitmap(await (await fetch(shot)).blob());
  const sx = image.width / rectangle.viewportWidth, sy = image.height / rectangle.viewportHeight;
  const canvas = new OffscreenCanvas(Math.round(rectangle.width * sx), Math.round(rectangle.height * sy));
  if (canvas.width * canvas.height > 25000000) { image.close(); throw new Error('截图过大，请缩小区域'); }
  canvas.getContext('2d').drawImage(image, rectangle.x * sx, rectangle.y * sy, rectangle.width * sx, rectangle.height * sy, 0, 0, canvas.width, canvas.height); image.close();
  const bytes = new Uint8Array(await (await canvas.convertToBlob({ type: 'image/png' })).arrayBuffer());
  if (bytes.length > 6 * 1024 * 1024) throw new Error('截图超过暂存限制，请缩小区域');
  await chrome.storage.session.set({ pendingItems: [{ title: `${tab.title} · 区域截图`, pageUrl: tab.url, mediaUrl: '', dataUrl: `data:image/png;base64,${base64(bytes)}`, kind: 'image' }], captureTab: tab });
  await panel();
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: 'capture-image', title: '保存图片至栈藏', contexts: ['image'], documentUrlPatterns: ['http://*/*', 'https://*/*'] });
    chrome.contextMenus.create({ id: 'capture-video', title: '保存视频至栈藏', contexts: ['video'], documentUrlPatterns: ['http://*/*', 'https://*/*'] });
    chrome.contextMenus.create({ id: 'capture-page', title: '栈藏：采集图片 / 区域截图', contexts: ['page'], documentUrlPatterns: ['http://*/*', 'https://*/*'] });
  });
});
chrome.contextMenus.onClicked.addListener((info, tab) => {
  const item = { tabId: tab?.id, pageUrl: info.pageUrl || tab?.url, title: tab?.title || '网页参考',
    mediaUrl: info.srcUrl || '', kind: info.menuItemId === 'capture-image' ? 'image' : 'video' };
  void (async () => {
    if (info.menuItemId === 'capture-page') { await chrome.storage.session.set({ captureTab: tab, pendingItems: [] }); await panel(); return; }
    if (item.kind === 'video' && tab?.id) {
      try {
        const [metadata] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: () => document.querySelector('video[poster]')?.poster || document.querySelector('meta[property="og:image"]')?.content || '' });
        if (metadata.result?.startsWith('http')) item.posterUrl = publicUrl(metadata.result);
      } catch { /* Some browsers disallow injection into protected frames. */ }
    }
    if (!item.mediaUrl.startsWith('http')) { if (item.kind === 'video') item.kind = 'link'; else throw new Error('此图片无法直接读取，请使用区域截图'); }
    const origins = [...new Set([item.mediaUrl, item.posterUrl].filter(url => url?.startsWith('http')).map(url => new URL(url).origin + '/*'))];
    if (origins.length && !(await chrome.permissions.contains({ origins }))) {
      await chrome.storage.session.set({ captureTab: tab, pendingItems: [item] }); await panel(); return;
    }
    await capture([item]);
  })().catch(error => progress({ busy: !!activeJob, error: true, message: error.message || String(error) }));
});

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  // Only our extension UI may initiate actions; injected page scripts cannot submit media IDs/bytes.
  if (sender.id !== chrome.runtime.id || sender.url !== chrome.runtime.getURL('popup.html')) return false;
  const action = async () => {
    switch (message.action) {
      case 'hello': {
        const saved = await chrome.storage.session.get('progress');
        if (!activeJob && saved.progress?.busy) await progress({ busy: false, error: true, message: '上次采集已中断，未确认保存成功；请查看收件箱或重新采集' });
        return native('hello');
      }
      case 'open': await native('hello'); return native('open');
      case 'capture': return capture(message.items, message.options);
      case 'screenshot': try { await screenshot(message.tabId); return {}; } catch(error) { await progress({busy:false,error:true,message:error.message}); await panel(); throw error; }
      case 'cancel': activeJob?.abort(); return {};
      default: throw new Error('未知采集操作');
    }
  };
  void action().then(value => respond({ ok: true, value })).catch(error => respond({ ok: false, error: error.message || String(error) }));
  return true;
});
