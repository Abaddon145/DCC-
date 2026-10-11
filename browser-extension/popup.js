import { publicUrl, folderPath } from './capture-utils.js';
const $ = id => document.getElementById(id);
let connection;
let items = [];
let tab;
let checking = false;
const send = async (action, payload = {}) => {
  const result = await chrome.runtime.sendMessage({ action, ...payload });
  if (!result?.ok) throw new Error(result?.error || '连接失败');
  return result.value;
};
function status(message) { $('status').textContent = message; }
function renderItems(values) {
  items = values.slice(0, 100); $('items').replaceChildren();
  items.forEach((item, index) => {
    const label = document.createElement('label'); const check = document.createElement('input');
    check.type = 'checkbox'; check.checked = true; check.dataset.index = String(index); check.addEventListener('change', update);
    const span = document.createElement('span'); span.textContent = item.title;
    const small = document.createElement('small'); small.textContent = item.kind === 'link' ? '来源链接 · 不下载视频文件' : (item.mediaUrl || '区域截图');
    span.append(small); label.append(check, span); $('items').append(label);
  });
  update();
}
function chosen() { return [...$('items').querySelectorAll('input:checked')].map(check => items[Number(check.dataset.index)]); }
function update() {
  const values = chosen(); $('selected-count').textContent = `${values.length} 项已选择`;
  $('capture').disabled = !connection?.enabled || !$('confirm').checked || !values.length;
  const kind = values[0]?.kind;
  const old = $('folder').value; $('folder').replaceChildren(new Option('收件箱 · 暂不分类', ''));
  for (const folder of connection?.folders || []) if (folder.kind === kind) $('folder').add(new Option(folderPath(folder, connection.folders), folder.id));
  $('folder').value = old;
  if (!$('folder').value) $('folder').value = '';
}
async function sourceTab() {
  const active = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (active[0]?.url?.startsWith('http')) { publicUrl(active[0].url); return active[0]; }
  const saved = (await chrome.storage.session.get('captureTab')).captureTab;
  if (saved?.id) { try { const current = await chrome.tabs.get(saved.id); if (current.url === saved.url) return current; } catch {} }
  const values = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const current = values.find(t => t.url?.startsWith('http'));
  if (!current) throw new Error('请从要采集的网页打开扩展');
  publicUrl(current.url); return current;
}
async function init() {
  if (checking) return;
  checking = true; $('reconnect').disabled = true;
  $('extension-id').textContent = chrome.runtime.id;
  try {
    $('connection').textContent = '正在连接栈藏…';
    const hello = await send('hello'); connection = hello.connection;
    $('connection').textContent = `目标素材库：${connection.name}${connection.enabled ? '' : '（采集已停用或素材库正在维护）'}${connection.choicesTruncated ? ' · 目标列表较多，仅展示部分；其他文件夹或项目请在应用中整理' : ''}`;
    $('confirm').checked = !hello.started && (await chrome.storage.local.get('confirmedLibrary')).confirmedLibrary === connection.libraryId;
    $('project').replaceChildren(new Option('不关联', ''));
    for (const project of connection.projects || []) $('project').add(new Option(project.name, project.id));
    const saved = await chrome.storage.session.get(['pendingItems', 'results', 'progress']);
    renderItems(saved.pendingItems || []); renderResults(saved.results || []);
    $('setup').open = false;
    status('连接成功，请确认目标素材库并选择需要保存的内容。');
    if (saved.progress) showProgress(saved.progress);
  } catch (error) {
    connection = null; $('confirm').checked = false; update();
    $('connection').textContent = '尚未连接软件'; $('setup').open = true;
    status(`请先在栈藏 → 采集收件箱点击“连接浏览器”，然后点击“重新检查连接”。\n连接详情：${error.message}`);
  } finally { checking = false; $('reconnect').disabled = false; }
}
function renderResults(results) {
  $('results').replaceChildren();
  for (const result of results) { const row = document.createElement('p'); row.textContent = `${result.title || '网页参考'}：${result.message}`; $('results').append(row); }
}
function showProgress(progress) {
  status(progress.message || ''); $('cancel').hidden = !progress.busy;
  if (progress.busy) $('capture').disabled = true; else update();
}
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== 'session') return;
  if (changes.progress) showProgress(changes.progress.newValue);
  if (changes.results) renderResults(changes.results.newValue || []);
});
$('confirm').addEventListener('change', async () => {
  if (connection && $('confirm').checked) await chrome.storage.local.set({ confirmedLibrary: connection.libraryId });
  else await chrome.storage.local.remove('confirmedLibrary'); update();
});
$('select-all').addEventListener('click', () => { const checks = [...$('items').querySelectorAll('input')]; const select = checks.some(c => !c.checked); for (const check of checks) check.checked = select; update(); });
$('page-images').addEventListener('click', () => void (async () => {
  tab = await sourceTab();
  const [result] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: () => {
    const seen = new Set(); const found = [];
    for (const image of document.images) {
      const src = image.currentSrc || image.src;
      if (!src.startsWith('http') || seen.has(src) || (image.naturalWidth && image.naturalWidth < 64)) continue;
      seen.add(src); found.push({ mediaUrl: src, title: image.alt || document.title }); if (found.length >= 100) break;
    }
    return found;
  } });
  const values = [];
  for (const item of result.result || []) { try { publicUrl(item.mediaUrl); values.push({ ...item, pageUrl: tab.url, kind: 'image' }); } catch {} }
  renderItems(values); status(values.length ? '勾选需要的图片。首次读取图片域名时会请求网站权限。' : '未发现可采集图片，可尝试区域截图。');
})().catch(error => status(error.message)));
$('page-link').addEventListener('click', () => void (async()=>{
  const value=await sourceTab(); let posterUrl='';
  try{const [metadata]=await chrome.scripting.executeScript({target:{tabId:value.id},func:()=>document.querySelector('meta[property="og:image"]')?.content||''});if(metadata.result?.startsWith('http'))posterUrl=publicUrl(metadata.result);}catch{}
  renderItems([{ title:value.title,pageUrl:value.url,mediaUrl:'',posterUrl,kind:'link' }]);
})().catch(error=>status(error.message)));
$('screenshot').addEventListener('click', () => void (async () => {
  tab = await sourceTab();
  await chrome.storage.session.set({ captureTab: tab });
  void send('screenshot', { tabId: tab.id }).catch(error => chrome.storage.session.set({ progress: { busy: false, error: true, message: error.message } }));
  window.close();
})().catch(error => status(error.message)));
$('capture').addEventListener('click', () => void (async () => {
  const values = chosen();
  const origins = [...new Set(values.flatMap(item => [item.mediaUrl,item.posterUrl]).filter(url => url?.startsWith('http')).map(url => new URL(publicUrl(url)).origin + '/*'))];
  // Request permissions inside the user's click, not from background page observation.
  if (origins.length && !(await chrome.permissions.request({ origins }))) throw new Error('未授权图片/视频域名，请使用区域截图或仅保存来源链接');
  $('capture').disabled = true;
  await send('capture', { items: values, options: { folderId: $('folder').value, projectId: $('project').value, tags: $('tags').value.split(/[;；]/).map(t => t.trim()).filter(Boolean) } });
})().catch(error => { status(error.message); update(); }));
$('cancel').addEventListener('click', () => void send('cancel').catch(error => status(error.message)));
$('copy-id').addEventListener('click', () => void navigator.clipboard.writeText(chrome.runtime.id)
  .then(() => status('扩展 ID 已复制，请粘贴到栈藏的“连接浏览器”。'))
  .catch(() => status('无法自动复制，请选中上方扩展 ID，按 Ctrl+C 复制。')));
$('reconnect').addEventListener('click', () => void init());
$('open-app').addEventListener('click', () => void (async () => {
  $('open-app').disabled = true;
  try { await send('open'); await init(); }
  catch (error) { $('setup').open = true; status(`首次连接需先从桌面或开始菜单打开栈藏，再连接浏览器。\n连接详情：${error.message}`); }
  finally { $('open-app').disabled = false; }
})());
void init();
