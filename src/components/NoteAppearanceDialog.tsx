import {useState} from "react";
import {defaultNoteAppearance,noteAppearancePresets,normalizeNoteAppearance,validateNoteAppearance,type NoteAppearance} from "../lib/noteWorkspace";
export function NoteAppearanceDialog({value,onPreview,onApply,onCancel}:{value:NoteAppearance;onPreview:(v:NoteAppearance)=>void;onApply:(v:NoteAppearance)=>Promise<void>;onCancel:()=>void}){
  const [draft,setDraft]=useState(value);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
  const change=(patch:Partial<NoteAppearance>)=>{const next=normalizeNoteAppearance({...draft,...patch});setDraft(next);onPreview(next);};
  return <div className="modal-backdrop" onPointerDown={e=>{if(e.target===e.currentTarget&&!busy)onCancel();}}><form className="modal-card note-appearance-dialog" role="dialog" aria-modal="true" aria-label="知识库外观" onKeyDown={e=>{if(e.key==="Escape"&&!busy)onCancel();}} onSubmit={e=>{e.preventDefault();try{validateNoteAppearance(draft);}catch(error){setError(String(error));return;}setBusy(true);void onApply(draft).catch(e=>setError(String(e))).finally(()=>setBusy(false));}}>
    <header><h2>知识库外观</h2><p>实时预览，应用后跨素材库共享。</p></header><div className="note-presets">{Object.entries(noteAppearancePresets).map(([name,preset])=><button type="button" key={name} onClick={()=>change(preset)}>{name}</button>)}</div>
    {([ ["background","背景色"],["text","正文颜色"],["accent","强调色"] ] as const).map(([key,label])=><label key={key}>{label}<input type="color" value={draft[key]} onChange={e=>change({[key]:e.target.value})}/></label>)}
    {([ ["fontSize","字号",12,24,1],["lineHeight","行距",1.3,2.5,.05],["contentWidth","正文宽度",540,1400,20],["navigationWidth","笔记本面板宽度",160,340,10],["listWidth","笔记列表宽度",220,440,10] ] as const).map(([key,label,min,max,step])=><label key={key}>{label}<input type="range" min={min} max={max} step={step} value={draft[key]} onChange={e=>change({[key]:Number(e.target.value)})}/><output>{draft[key]}</output></label>)}
    <label>代码块主题<select value={draft.codeTheme} onChange={e=>change({codeTheme:e.target.value as "dark"|"muted"})}><option value="dark">深色</option><option value="muted">柔和</option></select></label>{error&&<p role="alert">{error}</p>}
    <footer><button type="button" onClick={()=>change(defaultNoteAppearance)}>恢复默认</button><button type="button" disabled={busy} onClick={onCancel}>取消</button><button disabled={busy}>应用</button></footer>
  </form></div>;
}
