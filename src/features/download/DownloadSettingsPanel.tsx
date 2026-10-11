import {useEffect,useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";
import type {DownloadSettings,DownloadTool} from "../../types";
import "./download.css";
export function DownloadSettingsPanel({notify}:{notify:(message:string,error?:boolean)=>void}) {
 const [settings,setSettings]=useState<DownloadSettings|null>(null);const [tools,setTools]=useState<DownloadTool[]>([]);const [cookie,setCookie]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
 const load=async()=>{try{const [value,status,configured]=await Promise.all([api.getDownloadSettings(),api.getDownloadTools(),api.getDownloadCookieStatus()]);setSettings(value);setTools(status);setCookie(configured);setError("");}catch(err){setError(String(err));}};
 useEffect(()=>{void load();},[]);
 const mutate=async(action:()=>Promise<void>)=>{if(busy)return;setBusy(true);setError("");try{await action();notify("下载设置已保存");}catch(err){setError(String(err));}finally{setBusy(false);}};
 const patch=(value:Partial<DownloadSettings>)=>setSettings(previous=>previous?{...previous,...value}:null);
 const choose=async()=>{try{const path=await open({directory:true,multiple:false,title:"选择下载另存目录"});if(typeof path==="string")patch({directory:path});}catch(err){setError(String(err));}};
 const importCookie=async()=>{try{const path=await open({multiple:false,filters:[{name:"Cookie 文件",extensions:["txt"]}]});if(typeof path==="string")await mutate(async()=>{await api.importDownloadCookies(path);setCookie(true);});}catch(err){setError(String(err));}};
 return <section className="download-settings" aria-label="下载设置"><h2>下载设置</h2><p>下载完成后自动进入现有媒体库。工具仅由后端调用，不执行任意命令。</p>{error&&<p role="alert">{error}</p>}{!settings?<button onClick={()=>void load()}>重新读取</button>:<>
  <label><span>默认下载目录</span><input readOnly value={settings.directory} placeholder="仅入库时无需设置"/><button disabled={busy} onClick={()=>void choose()}>选择目录</button></label>
  <label><span>下载并发</span><select value={settings.concurrency} disabled={busy} onChange={event=>patch({concurrency:Number(event.target.value)})}>{[1,2,3,4].map(n=><option key={n} value={n}>{n} 项</option>)}</select></label>
  <label><span>默认质量</span><select value={settings.defaultQuality} disabled={busy} onChange={event=>patch({defaultQuality:event.target.value})}><option value="best">最佳</option><option value="1080p">1080P</option><option value="720p">720P</option></select></label>
  <label><span>代理地址</span><input value={settings.proxy} disabled={busy} onChange={event=>patch({proxy:event.target.value})} placeholder="留空为直连；如 http://127.0.0.1:7890"/></label>
  <label><span>Cookie 来源</span><select value={settings.cookieSource} disabled={busy} onChange={event=>patch({cookieSource:event.target.value as DownloadSettings["cookieSource"]})}><option value="none">不使用 Cookie</option><option value="file">已导入的安全 Cookie 文件</option><option value="edge">从 Edge 读取（开始解析/下载时）</option><option value="chrome">从 Chrome 读取（开始解析/下载时）</option></select></label>
  <p>{cookie?"已保存加密 Cookie":"尚未导入 Cookie"} · 使用 Windows 凭据管理器与 DPAPI 保护，不进入素材库备份。</p><div className="download-actions"><button disabled={busy} onClick={()=>void importCookie()}>导入 Cookie</button><button disabled={busy||!cookie} onClick={()=>{if(window.confirm("删除本机保存的下载 Cookie？"))void mutate(async()=>{await api.deleteDownloadCookies();setCookie(false);});}}>删除 Cookie</button><button className="primary-button" disabled={busy} onClick={()=>void mutate(()=>api.saveDownloadSettings(settings))}>保存下载设置</button></div>
 </>}<h3>工具状态</h3>{tools.map(tool=><p key={tool.name}><strong>{tool.name}</strong> · {tool.version||tool.message}<br/><small>{tool.path||"未找到组件"}</small></p>)}</section>;
}
