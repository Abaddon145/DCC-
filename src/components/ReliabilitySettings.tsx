import { useEffect,useRef,useState } from "react";
import { open,save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../lib/api";
import { readDiagnostics,recordDiagnostic } from "../lib/diagnostics";
import { useRequestGate } from "../lib/workspace";
import type { UpgradeBackupStatus } from "../types";
type Policy={enabled:boolean;directory:string;intervalHours:number;keep:number};
export function AutomaticBackupService({notify}:{notify:(text:string,error?:boolean)=>void}){
 const notifyRef=useRef(notify);notifyRef.current=notify;
 useEffect(()=>{if(!("__TAURI_INTERNALS__" in window))return;let alive=true;let running=false;let retryAfter=0;
 const check=async()=>{if(!alive||running||Date.now()<retryAfter)return;running=true;try{const path=await invoke<string|null>("get_automatic_backup_due");if(path&&alive){await api.exportBackup(path);await invoke("finish_automatic_backup",{path});notifyRef.current("自动备份已完成，仅轮换本软件生成的自动备份")}}catch{recordDiagnostic("save-failure");notifyRef.current("自动备份未完成，请检查备份目录、磁盘空间或任务中心；10 分钟后再检查",true);retryAfter=Date.now()+600000}finally{running=false}};
 const timer=setInterval(()=>void check(),60000);void check();return()=>{alive=false;clearInterval(timer)}},[]);return null;
}
export function ReliabilitySettings({notify,libraryId}:{libraryId:string;notify:(text:string,error?:boolean)=>void}){
 const [interrupted,setInterrupted]=useState(false);
 const [policy,setPolicy]=useState<Policy|null>(null),[error,setError]=useState(false),[busy,setBusy]=useState(false);
 const [upgrade,setUpgrade]=useState<UpgradeBackupStatus|null>(null),[upgradeError,setUpgradeError]=useState<string|null>(null),[keepText,setKeepText]=useState("5"),[upgradeBusy,setUpgradeBusy]=useState(false);
 const libraryRef=useRef(libraryId);libraryRef.current=libraryId;
 const upgradeSaving=useRef(false),upgradeGate=useRequestGate(),policyGate=useRequestGate();
 const load=()=>{
  const token=policyGate.begin(),id=libraryId;setError(false);setPolicy(null);
  void invoke<Policy>("get_automatic_backup_policy",{libraryId:id}).then(value=>{if(policyGate.isCurrent(token)&&libraryRef.current===id)setPolicy(value)}).catch(()=>{if(policyGate.isCurrent(token)&&libraryRef.current===id)setError(true)});
 };
 const loadUpgrade=()=>{
  const token=upgradeGate.begin(),id=libraryId;setUpgrade(null);setUpgradeError(null);
  void api.getUpgradeBackupStatus(id).then(value=>{if(upgradeGate.isCurrent(token)&&libraryRef.current===id){setUpgrade(value);setKeepText(String(value.keep))}}).catch(e=>{if(upgradeGate.isCurrent(token)&&libraryRef.current===id)setUpgradeError(String(e))});
 };
 useEffect(()=>{
  load();loadUpgrade();setInterrupted(false);setUpgradeBusy(false);upgradeSaving.current=false;
  let alive=true;
  void invoke<boolean>("get_interrupted_restore").then(value=>{if(alive)setInterrupted(value)}).catch(()=>{});
  return()=>{alive=false;upgradeGate.invalidate();policyGate.invalidate()};
 },[libraryId,upgradeGate,policyGate]);
 const keep=Number(keepText),validKeep=keepText.trim()!==""&&Number.isInteger(keep)&&keep>=1&&keep<=20;
 const saveUpgrade=async()=>{
  if(upgradeSaving.current||!validKeep||!upgrade||interrupted)return;
  const token=upgradeGate.begin(),id=libraryId;upgradeSaving.current=true;setUpgradeBusy(true);
  try{const value=await api.saveUpgradeBackupKeep(id,keep);if(upgradeGate.isCurrent(token)&&libraryRef.current===id){setUpgrade(value);setKeepText(String(value.keep));notify("升级保护设置已保存")}}
  catch(e){if(upgradeGate.isCurrent(token)&&libraryRef.current===id)notify(String(e),true)}
  finally{if(upgradeGate.isCurrent(token)&&libraryRef.current===id){upgradeSaving.current=false;setUpgradeBusy(false)}}
 };
 const savePolicy=async()=>{
  if(busy||!policy)return;const token=policyGate.begin(),id=libraryId;setBusy(true);
  try{const value=await invoke<Policy>("save_automatic_backup_policy",{...policy,libraryId:id});if(policyGate.isCurrent(token)&&libraryRef.current===id){setPolicy(value);notify("自动备份设置已保存")}}
  catch(e){if(policyGate.isCurrent(token)&&libraryRef.current===id)notify(String(e),true)}finally{setBusy(false)}
 };
 const exportLog=async()=>{if(busy)return;setBusy(true);try{const path=await save({defaultPath:"栈藏-脱敏诊断.json",filters:[{name:"JSON",extensions:["json"]}]});if(path){await invoke("export_diagnostics",{path,events:readDiagnostics()});notify("诊断已导出，不包含正文、文件路径、原始异常或凭据")}}catch(e){notify(String(e),true)}finally{setBusy(false)}};
 return <section>
  <h3>稳定性与恢复</h3>
  {interrupted&&<div className="read-failure"><span>检测到恢复中断。库内修改已暂停，原库副本仍保留。</span><button disabled={busy} onClick={()=>{if(!window.confirm("恢复到开始恢复备份之前的原库？中断操作安装的内容将被替换。"))return;setBusy(true);void invoke("recover_interrupted_restore").then(()=>window.location.reload()).catch(e=>notify(String(e),true)).finally(()=>setBusy(false))}}>确认恢复原库</button></div>}
  <p>未保存笔记提供独立恢复副本；中断在线任务需手动继续。</p>
  <button className="secondary-button" disabled={busy} onClick={()=>void exportLog()}>导出脱敏诊断</button>
  <h3>升级前数据库保护</h3>
  <p>每个素材库在应用新版本首次打开时生成校验快照；只含数据库，不含媒体，不是完整备份包。失败会提示并继续打开，下次重试。</p>
  {upgradeError?<div className="read-failure" role="alert"><span>升级保护状态读取失败：{upgradeError}</span><button onClick={loadUpgrade}>重新读取升级保护状态</button></div>:!upgrade?<p role="status">正在读取升级保护状态…</p>:<>
   <p>最近尝试：{upgrade.lastAttemptAt?new Date(upgrade.lastAttemptAt).toLocaleString():"尚无记录"} · 结果：{upgrade.result==="success"?"已校验":upgrade.result==="failed"?"未完成":"尚无记录"}</p>
   {upgrade.message&&<p>{upgrade.message}</p>}
   <p>快照目录：{upgrade.backupDirectory}</p>
   <label className="setting-row"><span>升级快照保留数量</span><input type="number" min={1} max={20} step={1} value={keepText} disabled={upgradeBusy||interrupted} onChange={e=>setKeepText(e.target.value)}/></label>
   <p>默认 5 份，允许 1–20；所有素材库共享此设置，仅轮换各库已登记的成功快照，不清理手动备份。</p>
   <button className="primary-button" disabled={upgradeBusy||interrupted||!validKeep} onClick={()=>void saveUpgrade()}>{upgradeBusy?"正在保存…":"保存升级保护设置"}</button>
  </>}
  <h3>自动完整备份</h3>
  {error?<button onClick={load}>重新读取备份设置</button>:!policy?<p role="status">正在读取备份设置…</p>:<>
   <label className="setting-row check"><span>启用自动备份（默认关闭）</span><input type="checkbox" checked={policy.enabled} onChange={e=>setPolicy({...policy,enabled:e.target.checked})}/></label>
   <button className="secondary-button" disabled={busy} onClick={()=>{const id=libraryId;void open({directory:true,multiple:false}).then(path=>{if(typeof path==="string"&&libraryRef.current===id)setPolicy({...policy,directory:path})})}}>选择备份目录</button>
   <p>{policy.directory||"尚未设置目录"}</p>
   <label className="setting-row"><span>间隔（小时）</span><input type="number" min={1} max={168} value={policy.intervalHours} onChange={e=>setPolicy({...policy,intervalHours:Number(e.target.value)})}/></label>
   <label className="setting-row"><span>保留自动备份数量</span><input type="number" min={1} max={20} value={policy.keep} onChange={e=>setPolicy({...policy,keep:Number(e.target.value)})}/></label>
   <p>仅在应用打开、素材库有变化且没有任务执行时备份；悬浮参考板打开时跳过。完整备份含媒体原件，仅清理当前库的旧自动备份，不清理手动备份。</p>
   <button className="primary-button" disabled={busy||(policy.enabled&&!policy.directory)} onClick={()=>void savePolicy()}>保存自动备份设置（立即生效）</button>
  </>}
 </section>;
}
