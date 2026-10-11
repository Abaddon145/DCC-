import {useEffect,useState} from "react";
import {LoaderCircle} from "lucide-react";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";
import type {DownloadSettings,DownloadTool} from "../../types";
import "./download.css";

export function DownloadSettingsPanel({notify}:{notify:(message:string,error?:boolean)=>void}) {
 const [settings,setSettings]=useState<DownloadSettings|null>(null);
 const [tools,setTools]=useState<DownloadTool[]>([]);
 const [cookie,setCookie]=useState(false);
 const [busy,setBusy]=useState(false);
 const [loading,setLoading]=useState(true);
 const [error,setError]=useState("");
 const load=async()=>{
  setLoading(true);setError("");
  try{const [value,status,configured]=await Promise.all([api.getDownloadSettings(),api.getDownloadTools(),api.getDownloadCookieStatus()]);setSettings(value);setTools(status);setCookie(configured);}
  catch(err){setError(String(err));}finally{setLoading(false);}
 };
 useEffect(()=>{void load();},[]);
 const mutate=async(action:()=>Promise<void>,message="下载设置已保存")=>{if(busy)return;setBusy(true);setError("");try{await action();notify(message);}catch(err){setError(String(err));}finally{setBusy(false);}};
 const patch=(value:Partial<DownloadSettings>)=>setSettings(previous=>previous?{...previous,...value}:null);
 const choose=async()=>{try{const path=await open({directory:true,multiple:false,title:"选择下载另存目录"});if(typeof path==="string")patch({directory:path});}catch(err){setError(String(err));}};
 const importCookie=async()=>{try{const path=await open({multiple:false,filters:[{name:"Cookie 文件",extensions:["txt"]}]});if(typeof path==="string")await mutate(async()=>{await api.importDownloadCookies(path);setCookie(true);},"Cookie 已加密保存");}catch(err){setError(String(err));}};
 return <section className="download-settings" aria-label="下载设置" aria-busy={loading||busy}>
  <header><h2>下载设置</h2><p>下载完成后自动进入现有媒体库。工具仅由后端调用，不执行任意命令。</p></header>
  {error&&<p className="download-error" role="alert">{error}</p>}
  {loading?<p className="download-loading" role="status"><LoaderCircle size={18} className="download-spinner" aria-hidden="true"/>正在读取下载设置与组件状态…</p>:!settings?<button className="secondary-button" onClick={()=>void load()}>重新读取</button>:<>
   <fieldset className="download-settings-group" disabled={busy}><legend>保存与下载</legend>
    <div className="download-settings-row"><label htmlFor="download-directory">默认下载目录</label><div className="download-directory-control"><input id="download-directory" readOnly value={settings.directory} placeholder="仅入库时无需设置"/><button className="secondary-button" onClick={()=>void choose()}>选择目录</button></div></div>
    <div className="download-settings-row"><label htmlFor="download-concurrency">下载并发</label><select id="download-concurrency" value={settings.concurrency} onChange={event=>patch({concurrency:Number(event.target.value)})}>{[1,2,3,4].map(n=><option key={n} value={n}>{n} 项</option>)}</select></div>
    <div className="download-settings-row"><label htmlFor="download-quality">默认质量</label><select id="download-quality" value={settings.defaultQuality} onChange={event=>patch({defaultQuality:event.target.value})}><option value="best">最佳</option><option value="1080p">1080P</option><option value="720p">720P</option></select></div>
    <p className="download-hint">仅入库时无需另存目录；原件将托管到当前素材库。</p>
   </fieldset>
   <fieldset className="download-settings-group" disabled={busy}><legend>网络与 Cookie</legend>
    <div className="download-settings-row"><label htmlFor="download-proxy">代理地址</label><input id="download-proxy" value={settings.proxy} onChange={event=>patch({proxy:event.target.value})} placeholder="留空为直连；如 http://127.0.0.1:7890"/></div>
    <div className="download-settings-row"><label htmlFor="download-cookie">Cookie 来源</label><select id="download-cookie" value={settings.cookieSource} onChange={event=>patch({cookieSource:event.target.value as DownloadSettings["cookieSource"]})}><option value="none">不使用 Cookie</option><option value="file">已导入的安全 Cookie 文件</option><option value="edge">从 Edge 读取（开始解析/下载时）</option><option value="chrome">从 Chrome 读取（开始解析/下载时）</option></select></div>
    <p className="download-hint">{cookie?"已保存加密 Cookie":"尚未导入 Cookie"} · 使用 Windows 凭据管理器与 DPAPI 保护，不进入素材库备份。</p>
    <div className="download-actions"><button className="secondary-button" onClick={()=>void importCookie()}>导入 Cookie</button><button className="secondary-button danger" disabled={!cookie} onClick={()=>{if(window.confirm("删除本机保存的下载 Cookie？"))void mutate(async()=>{await api.deleteDownloadCookies();setCookie(false);},"Cookie 已删除");}}>删除 Cookie</button></div>
   </fieldset>
   <div className="download-settings-save"><button className="primary-button" disabled={busy} onClick={()=>void mutate(()=>api.saveDownloadSettings(settings))}>{busy&&<LoaderCircle size={16} className="download-spinner" aria-hidden="true"/>}{busy?"正在处理…":"保存下载设置"}</button></div>
  </>}
  {!loading&&tools.length>0&&<section className="download-settings-group" aria-label="工具状态"><h3>工具状态</h3><p className="download-hint">由软件管理，用于解析、下载和生成预览。</p>{tools.map(tool=><article className="download-tool" key={tool.name}>
   <div><strong>{tool.name}</strong><span className={`download-tool-status ${tool.ready?"ready":"missing"}`}>{tool.ready?"可用":"不可用"}</span></div>
   <p>{tool.version||tool.message}</p><details><summary>组件路径</summary><code>{tool.path||"未找到组件"}</code></details>
  </article>)}</section>}
 </section>;
}
