import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Cloud, Database, ExternalLink, Eye, FolderInput, FolderOpen, Grid2X2, Keyboard, Languages, LayoutGrid, LibraryBig, List, LogIn, Play, Plus, Power, RotateCcw, SlidersHorizontal, Trash2, Unplug, X } from "lucide-react";
import { api } from "../lib/api";
import type { BaiduDeviceAuthorization, BaiduNetdiskSettings, GlobalPreferences, LibraryLocationState, LibraryPreferences, PersonalizationState, StorageChangeRequest, TranslationSettings } from "../types";
import { formatBytes } from "../lib/validation";
import { applyGlobalPreferences, commandRegistry, defaultGlobalPreferences, defaultLibraryPreferences, isHexColor, moduleRegistry, shortcutConflicts } from "../lib/personalization";

type SettingsTab = "appearance" | "view" | "modules" | "shortcuts" | "startup" | "online" | "data";
interface Props { onClose: () => void; onLibraryChanged: () => Promise<void>; notify: (message: string, error?: boolean) => void; libraryLocked?: boolean; initialTab?: SettingsTab; onPersonalizationChanged?: (value: PersonalizationState) => void }

const tabs: Array<{ id: SettingsTab; label: string; icon: typeof Eye }> = [
  { id: "appearance", label: "外观", icon: Eye }, { id: "view", label: "素材视图", icon: LayoutGrid },
  { id: "modules", label: "功能模块", icon: SlidersHorizontal }, { id: "shortcuts", label: "快捷键", icon: Keyboard },
  { id: "startup", label: "启动行为", icon: Play }, { id: "online", label: "在线服务", icon: Languages },
  { id: "data", label: "数据与素材库", icon: Database },
];

export function StorageSettings({ onClose, onLibraryChanged, notify, libraryLocked = false, initialTab = "online", onPersonalizationChanged }: Props) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [state, setState] = useState<LibraryLocationState | null>(null);
  const [busy, setBusy] = useState(false);
  const [translation, setTranslation] = useState<TranslationSettings | null>(null);
  const [appId, setAppId] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [translationBusy, setTranslationBusy] = useState(false);
  const [netdisk, setNetdisk] = useState<BaiduNetdiskSettings | null>(null);
  const [netdiskAppId, setNetdiskAppId] = useState("");
  const [netdiskAppKey, setNetdiskAppKey] = useState("");
  const [netdiskSecret, setNetdiskSecret] = useState("");
  const [netdiskAuth, setNetdiskAuth] = useState<BaiduDeviceAuthorization | null>(null);
  const [netdiskBusy, setNetdiskBusy] = useState(false);
  const [saved, setSaved] = useState<PersonalizationState | null>(null);
  const [global, setGlobal] = useState<GlobalPreferences>(defaultGlobalPreferences);
  const [library, setLibrary] = useState<LibraryPreferences>(defaultLibraryPreferences);
  const [dragModule, setDragModule] = useState<string | null>(null);

  const load = () => api.getLibraryLocations().then(setState).catch(error => notify(String(error), true));
  const loadTranslation = () => api.getTranslationSettings().then(setTranslation).catch(error => notify(String(error), true));
  const loadNetdisk = () => api.getBaiduNetdiskSettings().then(setNetdisk).catch(error => notify(String(error), true));
  useEffect(() => {
    void load(); void loadTranslation(); void loadNetdisk();
    void api.getPersonalization?.().then(value => { setSaved(value); setGlobal(value.global); setLibrary(value.library); applyGlobalPreferences(value.global); }).catch(() => undefined);
  }, []);
  useEffect(() => { applyGlobalPreferences(global); }, [global]);

  const closeAndRestore = () => { if (saved) applyGlobalPreferences(saved.global); onClose(); };
  const conflicts = useMemo(() => shortcutConflicts(global.shortcuts), [global.shortcuts]);
  const canApply = isHexColor(global.accentColor) && conflicts.size === 0;
  const apply = async () => {
    if (!canApply) { notify("请先修正强调色或快捷键冲突", true); return; }
    setBusy(true);
    try {
      const [nextGlobal, nextLibrary] = await Promise.all([api.saveGlobalPreferences(global), api.saveLibraryPreferences(library)]);
      const value = { global: nextGlobal, library: nextLibrary }; setSaved(value); onPersonalizationChanged?.(value); applyGlobalPreferences(nextGlobal); notify("个性化设置已应用"); onClose();
    } catch (error) { notify(`保存设置失败：${String(error)}`, true); } finally { setBusy(false); }
  };

  const saveCredentials = async () => {
    if (!appId.trim() || !secretKey.trim()) { notify("请输入百度翻译 APP ID 和密钥", true); return; }
    setTranslationBusy(true);
    try { await api.saveTranslationCredentials(appId, secretKey); setSecretKey(""); await loadTranslation(); notify("翻译凭据已安全保存到 Windows 凭据管理器"); }
    catch (error) { notify(String(error), true); } finally { setTranslationBusy(false); }
  };
  const hasEnteredCredentials = Boolean(appId.trim() && secretKey.trim());
  const testTranslation = async () => {
    setTranslationBusy(true);
    try { if (hasEnteredCredentials) { await api.saveTranslationCredentials(appId, secretKey); await loadTranslation(); } const result = await api.testTranslationService(); if (hasEnteredCredentials) setSecretKey(""); notify(result.message); }
    catch (error) { notify(String(error), true); } finally { setTranslationBusy(false); }
  };
  const removeCredentials = async () => { if (!window.confirm("删除本机保存的百度翻译凭据？")) return; try { await api.deleteTranslationCredentials(); await loadTranslation(); notify("翻译凭据已删除"); } catch (error) { notify(String(error), true); } };

  const hasNetdiskCredentials = Boolean(netdiskAppId.trim() && netdiskAppKey.trim() && netdiskSecret.trim());
  const saveNetdiskCredentials = async () => {
    if (!hasNetdiskCredentials) { notify("请输入百度网盘应用的 App ID、App Key 和 Secret Key", true); return false; }
    setNetdiskBusy(true);
    try {
      const next = await api.saveBaiduNetdiskCredentials(netdiskAppId.trim(), netdiskAppKey.trim(), netdiskSecret);
      setNetdisk(next); setNetdiskSecret(""); notify("百度网盘应用凭据已安全保存"); return true;
    } catch (error) { notify(String(error), true); return false; } finally { setNetdiskBusy(false); }
  };
  const startNetdiskAuthorization = async () => {
    setNetdiskBusy(true);
    try {
      if (hasNetdiskCredentials) {
        const next = await api.saveBaiduNetdiskCredentials(netdiskAppId.trim(), netdiskAppKey.trim(), netdiskSecret);
        setNetdisk(next); setNetdiskSecret("");
      } else if (!netdisk?.configured) { notify("请先填写并保存百度网盘应用凭据", true); return; }
      const auth = await api.startBaiduNetdiskAuthorization(); setNetdiskAuth(auth);
      await api.openExternal(auth.qrcodeUrl || auth.verificationUrl);
      notify("已打开百度官方授权页，授权后请返回点击“我已授权”");
    } catch (error) { notify(String(error), true); } finally { setNetdiskBusy(false); }
  };
  const completeNetdiskAuthorization = async () => {
    setNetdiskBusy(true);
    try { const next = await api.completeBaiduNetdiskAuthorization(); setNetdisk(next); setNetdiskAuth(null); notify(`百度网盘已连接${next.accountName ? `：${next.accountName}` : ""}`); }
    catch (error) { notify(String(error), true); } finally { setNetdiskBusy(false); }
  };
  const disconnectNetdisk = async () => { try { setNetdisk(await api.disconnectBaiduNetdisk()); setNetdiskAuth(null); notify("百度网盘账号已断开"); } catch (error) { notify(String(error), true); } };
  const deleteNetdiskCredentials = async () => { if (!window.confirm("删除本机保存的百度网盘应用凭据和登录令牌？")) return; try { await api.deleteBaiduNetdiskCredentials(); await loadNetdisk(); setNetdiskAuth(null); notify("百度网盘配置已删除"); } catch (error) { notify(String(error), true); } };

  const choose = async (mode: StorageChangeRequest["mode"], existingPath?: string) => {
    const selected = existingPath || await open({ directory: true, multiple: false, title: mode === "open" ? "选择已有栈藏素材库" : "选择空目录" });
    if (typeof selected !== "string") return;
    let name: string | undefined;
    if (mode === "create") { name = window.prompt("素材库名称", "新素材库")?.trim(); if (!name) return; }
    if (mode === "migrate" && !window.confirm("将完整复制当前素材库并切换到新目录。旧库会保留，是否继续？")) return;
    setBusy(true);
    try { const next = await api.changeLibrary({ mode, path: selected, name }); setState(next); await onLibraryChanged(); notify(mode === "migrate" ? "素材库已复制并切换" : mode === "create" ? "新素材库已创建" : "素材库已切换"); }
    catch (error) { notify(String(error), true); } finally { setBusy(false); }
  };
  const forget = async (path: string) => { try { await api.forgetRecentLibrary(path); void load(); } catch (error) { notify(String(error), true); } };
  const updateGlobal = <K extends keyof GlobalPreferences>(key: K, value: GlobalPreferences[K]) => setGlobal(previous => ({ ...previous, [key]: value }));
  const updateLibrary = <K extends keyof LibraryPreferences>(key: K, value: LibraryPreferences[K]) => setLibrary(previous => ({ ...previous, [key]: value }));
  const moveModule = (target: string) => {
    if (!dragModule || dragModule === "library" || target === "library" || dragModule === target) return;
    const next = library.moduleOrder.filter(id => id !== dragModule); next.splice(next.indexOf(target), 0, dragModule); updateLibrary("moduleOrder", next); setDragModule(null);
  };

  return <div className="modal-backdrop"><section className="settings-modal settings-center" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">SETTINGS CENTER</span><h2>设置中心</h2></div><button className="icon-button" onClick={closeAndRestore}><X size={19} /></button></header>
    <div className="settings-center-layout">
      <nav className="settings-nav">{tabs.map(item => { const Icon = item.icon; return <button key={item.id} className={tab === item.id ? "active" : ""} onClick={() => setTab(item.id)}><Icon size={16} />{item.label}</button>; })}</nav>
      <div className="settings-body settings-page">
        {tab === "appearance" && <><SettingsTitle title="外观" text="主题、强调色与整体界面尺寸会实时预览。" />
          <section><h3>主题预设</h3><div className="theme-cards">{(["graphite", "ue-slate", "midnight"] as const).map((theme, index) => <button key={theme} className={global.theme === theme ? "active" : ""} onClick={() => updateGlobal("theme", theme)}><i className={`theme-swatch ${theme}`} /><strong>{["Graphite", "UE Slate", "Midnight"][index]}</strong></button>)}</div></section>
          <section><h3>强调色</h3><div className="setting-row"><label><span>自定义颜色</span><small>使用 #RRGGBB 格式</small></label><div className="color-setting"><input type="color" value={isHexColor(global.accentColor) ? global.accentColor : "#D99A42"} onChange={event => updateGlobal("accentColor", event.target.value.toUpperCase())} /><input aria-label="强调色" value={global.accentColor} onChange={event => updateGlobal("accentColor", event.target.value.toUpperCase())} className={!isHexColor(global.accentColor) ? "invalid" : ""} /></div></div></section>
          <section><h3>布局与动效</h3><SelectRow label="界面密度" value={global.density} onChange={value => updateGlobal("density", value as GlobalPreferences["density"])} options={[["comfortable","舒适"],["compact","紧凑"]]} /><RangeRow label="侧栏宽度" value={global.sidebarWidth} min={210} max={360} onChange={value => updateGlobal("sidebarWidth", value)} /><RangeRow label="详情面板宽度" value={global.detailWidth} min={340} max={640} onChange={value => updateGlobal("detailWidth", value)} /><CheckRow label="减少动画" checked={global.reduceMotion} onChange={value => updateGlobal("reduceMotion", value)} /></section>
        </>}
        {tab === "view" && <><SettingsTitle title="素材视图" text="这些选项只影响当前素材库。" /><section><h3>显示方式</h3><div className="segmented setting-segment"><button className={library.assetView === "grid" ? "active" : ""} onClick={() => updateLibrary("assetView", "grid")}><Grid2X2 size={16} />网格</button><button className={library.assetView === "list" ? "active" : ""} onClick={() => updateLibrary("assetView", "list")}><List size={16} />紧凑列表</button></div><SelectRow label="卡片尺寸" value={library.cardSize} onChange={value => updateLibrary("cardSize", value as LibraryPreferences["cardSize"])} options={[["small","小"],["medium","中"],["large","大"]]} /><SelectRow label="封面适应" value={library.coverFit} onChange={value => updateLibrary("coverFit", value as LibraryPreferences["coverFit"])} options={[["cover","填满裁切"],["contain","完整显示"]]} /></section><section><h3>卡片信息</h3>{Object.entries({ category:"分类",tags:"标签",software:"软件",version:"版本",format:"格式",linkStatus:"链接状态" }).map(([key,label]) => <CheckRow key={key} label={label} checked={library.cardFields[key as keyof typeof library.cardFields]} onChange={value => updateLibrary("cardFields", { ...library.cardFields, [key]: value })} />)}</section></>}
        {tab === "modules" && <><SettingsTitle title="功能模块" text="拖拽调整侧栏顺序；关闭模块不会删除其中的数据。" /><section><div className="module-list">{library.moduleOrder.map(id => { const module = moduleRegistry.find(item => item.id === id); if (!module) return null; const locked = id === "library" || (id === "reference" && libraryLocked); const enabled = id === "library" || !library.disabledModules.includes(id); return <div key={id} draggable={!locked} onDragStart={() => setDragModule(id)} onDragOver={event => event.preventDefault()} onDrop={() => moveModule(id)}><span className="drag-handle">⋮⋮</span><div><strong>{module.label}</strong><small>{module.description}{id === "reference" && libraryLocked ? " · 悬浮窗口使用中" : ""}</small></div><button className={`module-toggle ${enabled ? "active" : ""}`} disabled={locked} onClick={() => updateLibrary("disabledModules", enabled ? [...library.disabledModules, id] : library.disabledModules.filter(value => value !== id))}><Power size={14} />{enabled ? "已启用" : "已关闭"}</button></div>; })}</div></section></>}
        {tab === "shortcuts" && <><SettingsTitle title="快捷键" text="点击输入框后直接按下新的组合键；重复组合会被标记。" /><section><div className="shortcut-list">{commandRegistry.map(command => <label key={command.id} className={conflicts.has(command.id) ? "conflict" : ""}><span>{command.label}</span><input readOnly value={global.shortcuts[command.id] || ""} onKeyDown={event => { event.preventDefault(); const parts = [event.ctrlKey || event.metaKey ? "Ctrl" : "", event.altKey ? "Alt" : "", event.shiftKey ? "Shift" : "", !["Control","Shift","Alt","Meta"].includes(event.key) ? (event.key.length === 1 ? event.key.toUpperCase() : event.key) : ""].filter(Boolean); if (parts.length && !["Ctrl","Alt","Shift"].includes(parts.at(-1)!)) updateGlobal("shortcuts", { ...global.shortcuts, [command.id]: parts.join("+") }); }} /><button title="恢复默认" onClick={() => updateGlobal("shortcuts", { ...global.shortcuts, [command.id]: defaultGlobalPreferences.shortcuts[command.id] })}><RotateCcw size={13} /></button></label>)}</div>{conflicts.size > 0 && <div className="settings-warning">存在重复快捷键，请修改后再应用。</div>}<button className="secondary-button" onClick={() => updateGlobal("shortcuts", { ...defaultGlobalPreferences.shortcuts })}><RotateCcw size={14} />全部恢复默认</button></section></>}
        {tab === "startup" && <><SettingsTitle title="启动行为" text="控制当前素材库打开时的初始状态。" /><section><SelectRow label="启动模块" value={library.startupModule} onChange={value => updateLibrary("startupModule", value)} options={moduleRegistry.filter(item => item.id === "library" || !library.disabledModules.includes(item.id)).map(item => [item.id,item.label])} /><SelectRow label="默认排序" value={library.defaultSort} onChange={value => updateLibrary("defaultSort", value as LibraryPreferences["defaultSort"])} options={[["updated","最近更新"],["name","名称"],["created","创建时间"],["recent","最近查看"],["favorite","收藏优先"]]} /><CheckRow label="记忆上次打开的模块" checked={library.rememberLastContext} onChange={value => updateLibrary("rememberLastContext", value)} /><CheckRow label="记忆最后一次搜索和筛选" checked={library.rememberSearch} onChange={value => updateLibrary("rememberSearch", value)} /><CheckRow label="分类树默认展开" checked={library.categoryTreeExpanded} onChange={value => updateLibrary("categoryTreeExpanded", value)} /></section></>}
        {tab === "online" && <><SettingsTitle title="在线服务" text="除翻译、Fab、链接检查和网盘转存外，软件可以完全离线使用。" />
          <section><h3>在线翻译</h3><div className="translation-settings"><div className="translation-settings-title"><Languages size={20} /><div><strong>百度翻译开放平台</strong><span>{translation?.configured ? "凭据已配置" : "尚未配置凭据"} · 密钥不会进入素材库或备份</span></div></div><div className="credential-grid"><label><span>APP ID</span><input aria-label="APP ID" value={appId} onChange={event => setAppId(event.target.value)} placeholder={translation?.configured ? "输入新的 APP ID 可替换凭据" : "百度翻译 APP ID"} /></label><label><span>密钥</span><input aria-label="密钥" type="password" value={secretKey} onChange={event => setSecretKey(event.target.value)} placeholder="不会回显已保存密钥" /></label></div><p>翻译时仅将名称、描述、标签和许可文字发送给百度翻译；首次配置可直接填写两项后点击“保存并测试”。</p><CheckRow label="Fab 导入后自动补齐缺失的中文内容" checked={translation?.fabAutoTranslate ?? true} onChange={async enabled => { try { await api.setFabAutoTranslate(enabled); setTranslation(current => current ? { ...current, fabAutoTranslate: enabled } : current); } catch (error) { notify(String(error), true); } }} /><div className="translation-settings-actions"><button className="primary-button" disabled={translationBusy || !hasEnteredCredentials} onClick={saveCredentials}>保存凭据</button><button className="secondary-button" disabled={translationBusy || (!translation?.configured && !hasEnteredCredentials)} onClick={testTranslation}>{hasEnteredCredentials ? "保存并测试" : "测试连接"}</button><button className="secondary-button danger" disabled={!translation?.configured || translationBusy} onClick={removeCredentials}>删除凭据</button></div></div></section>
          <section><h3>百度网盘账号</h3><div className="translation-settings netdisk-settings"><div className="translation-settings-title"><Cloud size={20} /><div><strong>百度网盘开放平台</strong><span>{netdisk?.connected ? `已连接${netdisk.accountName ? ` · ${netdisk.accountName}` : ""}` : netdisk?.configured ? "应用凭据已配置，等待账号授权" : "尚未配置"} · 凭据和令牌仅保存在 Windows 凭据管理器</span></div></div><div className="credential-grid netdisk-credentials"><label><span>App ID</span><input aria-label="网盘 App ID" value={netdiskAppId} onChange={event => setNetdiskAppId(event.target.value)} placeholder={netdisk?.configured ? "输入新值可替换配置" : "百度网盘 App ID"} /></label><label><span>App Key</span><input aria-label="网盘 App Key" value={netdiskAppKey} onChange={event => setNetdiskAppKey(event.target.value)} placeholder={netdisk?.configured ? "输入新值可替换配置" : "百度网盘 App Key"} /></label><label><span>Secret Key</span><input aria-label="网盘 Secret Key" type="password" value={netdiskSecret} onChange={event => setNetdiskSecret(event.target.value)} placeholder="不会回显已保存密钥" /></label></div><p>使用百度官方设备码授权；软件不保存账号密码。授权后可在“网盘保存助手”中浏览目录并直接转存分享文件。应用须具备网盘文件列表、分享访问与转存权限。</p>{netdiskAuth && <div className="netdisk-auth"><strong>授权码：{netdiskAuth.userCode}</strong><span>请在百度官方页面确认授权，然后返回点击“我已授权”。</span><button className="secondary-button" onClick={() => void api.openExternal(netdiskAuth.qrcodeUrl || netdiskAuth.verificationUrl)}><ExternalLink size={14} />重新打开授权页</button></div>}<div className="translation-settings-actions"><button className="secondary-button" disabled={netdiskBusy || !hasNetdiskCredentials} onClick={() => void saveNetdiskCredentials()}>保存应用配置</button><button className="primary-button" disabled={netdiskBusy || (!netdisk?.configured && !hasNetdiskCredentials)} onClick={() => void startNetdiskAuthorization()}><LogIn size={14} />{netdisk?.connected ? "重新授权" : "登录百度网盘"}</button>{netdiskAuth && <button className="primary-button" disabled={netdiskBusy} onClick={() => void completeNetdiskAuthorization()}>我已授权</button>}<button className="secondary-button" disabled={netdiskBusy || !netdisk?.connected} onClick={() => void disconnectNetdisk()}><Unplug size={14} />断开账号</button><button className="secondary-button danger" disabled={netdiskBusy || !netdisk?.configured} onClick={() => void deleteNetdiskCredentials()}>删除配置</button></div></div></section>
        </>}
        {tab === "data" && <><SettingsTitle title="数据与素材库" text="管理当前素材库位置、迁移与最近库。" />{state?.startupWarning && <div className="settings-warning">{state.startupWarning}</div>}{state && <section><h3>当前素材库</h3><div className="current-library"><div className="library-icon"><Database size={24} /></div><div><strong>{state.current.name}</strong><span title={state.current.path}>{state.current.path}</span><small>{formatBytes(state.current.sizeBytes)} · 库 ID {state.current.libraryId.slice(0, 8)}</small></div></div></section>}<section><h3>更改存储位置</h3>{libraryLocked && <div className="settings-warning">参考板正在悬浮窗口中，请先收回后再切换或迁移素材库。</div>}<div className="storage-actions"><button onClick={() => void choose("migrate")} disabled={busy || libraryLocked}><FolderInput size={18} /><strong>复制并迁移</strong><span>完整复制当前库，保留旧目录</span></button><button onClick={() => void choose("create")} disabled={busy || libraryLocked}><Plus size={18} /><strong>创建新库</strong><span>在空目录创建独立素材库</span></button><button onClick={() => void choose("open")} disabled={busy || libraryLocked}><FolderOpen size={18} /><strong>打开已有库</strong><span>验证后切换到已有素材库</span></button></div></section><section><h3>最近使用</h3><div className="recent-libraries">{state?.recent.length ? state.recent.map(item => <div key={item.path} className={!item.available ? "unavailable" : ""}><LibraryBig size={17} /><div><strong>{item.name}{item.retainedCopy && <em>旧库副本</em>}</strong><span title={item.path}>{item.path}</span></div><button disabled={!item.available || busy || libraryLocked} onClick={() => void choose("open", item.path)}>打开</button><button className="icon-button subtle" onClick={() => void forget(item.path)} title="从列表移除"><Trash2 size={14} /></button></div>) : <p className="empty-mini">暂无其他素材库</p>}</div></section></>}
      </div>
    </div>
    <footer className="modal-footer"><button className="secondary-button" onClick={() => { setGlobal(defaultGlobalPreferences); setLibrary(defaultLibraryPreferences); notify("默认设置已载入，点击应用后保存"); }}><RotateCcw size={14} />恢复默认</button><span className="grow" /><button className="secondary-button" onClick={closeAndRestore}>取消</button><button className="primary-button" disabled={busy || !canApply} onClick={() => void apply()}>应用</button></footer>
  </section></div>;
}

function SettingsTitle({ title, text }: { title: string; text: string }) { return <header className="settings-page-title"><h2>{title}</h2><p>{text}</p></header>; }
function CheckRow({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) { return <label className="setting-row check"><span>{label}</span><input type="checkbox" checked={checked} onChange={event => onChange(event.target.checked)} /></label>; }
function SelectRow({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: string[][] }) { return <label className="setting-row"><span>{label}</span><select value={value} onChange={event => onChange(event.target.value)}>{options.map(([id,text]) => <option key={id} value={id}>{text}</option>)}</select></label>; }
function RangeRow({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (value: number) => void }) { return <label className="setting-row"><span>{label}<small>{value}px</small></span><input type="range" min={min} max={max} value={value} onChange={event => onChange(Number(event.target.value))} /></label>; }
