import {useEffect,useRef,useState} from "react";
import {Extension,Node,NodeViewWrapper,ReactNodeViewRenderer,mergeAttributes,type Extensions,type NodeViewProps,type JSONContent} from "@tiptap/react";
import {Plugin} from "@tiptap/pm/state";
import type {Node as DocumentNode} from "@tiptap/pm/model";
import {BookOpen,GripVertical,Play} from "lucide-react";
import {learningTime} from "../features/learning/LearningControls";
export interface LearningReferenceData {courseId:string;mediaId:string;fingerprint:string;ranges:Array<{startMs:number;endMs:number}>;label:string}
export interface LearningNoteOptions {readPreserved?:(id:string)=>Promise<string>;openReference?:(reference:LearningReferenceData,index:number)=>Promise<void>;confirmDelete?:(message:string)=>boolean}
const escapedLabel=(label:unknown)=>String(label||"学习资料").replace(/[\\\[\]]/g,"\\$&");
export function referenceMarkdown(data:LearningReferenceData):string {
  return `[${escapedLabel(data.label)}](dcclearning://${data.courseId}?media=${data.mediaId}&sha256=${data.fingerprint}&ranges=${data.ranges.map(range=>`${range.startMs}-${range.endMs}`).join(",")})`;
}
function LearningAtom({node,extension}:NodeViewProps) {
  const options=extension.options as LearningNoteOptions;
  const preserved=node.type.name==="preservedMarkdown";
  const {courseId,mediaId,fingerprint,ranges:storedRanges,label}=node.attrs;
  const reference:LearningReferenceData={courseId,mediaId,fingerprint,ranges:storedRanges,label};
  const identity=preserved?String(node.attrs.archiveItemId):JSON.stringify([reference.courseId,reference.mediaId,reference.fingerprint,reference.ranges]);
  const [raw,setRaw]=useState<string|null>(null),[expanded,setExpanded]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const generation=useRef(0);
  useEffect(()=>{generation.current++;setRaw(null);setExpanded(false);setBusy(false);setError("");return()=>{generation.current++;};},[identity]);
  const run=async(work:()=>Promise<void>)=>{if(busy)return;const token=++generation.current;setBusy(true);setError("");try{await work();}catch(reason){if(token===generation.current)setError(String(reason));}finally{if(token===generation.current)setBusy(false);}};
  const read=()=>{if(!options.readPreserved)return;const id=String(node.attrs.archiveItemId);const token=generation.current+1;void run(async()=>{const value=await options.readPreserved!(id);if(generation.current===token){setRaw(value);setExpanded(true);}});};
  const ranges=Array.isArray(reference.ranges)?reference.ranges:[];
  const [page,setPage]=useState(0);
  useEffect(()=>setPage(0),[identity]);
  return <NodeViewWrapper className="knowledge-learning-atom" contentEditable={false}>
    <div className="knowledge-learning-atom-heading"><span data-drag-handle title="拖动学习内容块"><GripVertical size={15}/></span>{preserved?<BookOpen size={16}/>:<Play size={16}/>}<strong>{String(node.attrs.label||"学习资料")}</strong></div>
    {preserved?<><p>已保留原始 Markdown，仅查看，不执行其中的 HTML。</p><button type="button" className="secondary-button" disabled={busy||!options.readPreserved} onClick={()=>expanded?setExpanded(false):read()}>{expanded?"收起原文":error?"重试读取原文":"查看保留原文"}</button>
      {expanded&&raw!==null&&<textarea aria-label="保留的 Markdown 原文" readOnly value={raw} spellCheck={false} onCopy={event=>event.stopPropagation()} onCut={event=>event.stopPropagation()} onPaste={event=>event.stopPropagation()} onContextMenu={event=>event.stopPropagation()}/>}</>
      :<><p>{ranges.length} 个时间片段 · 打开前校验来源</p><div className="knowledge-learning-ranges">{ranges.slice(page*20,page*20+20).map((range,index)=><button key={page*20+index} type="button" className="secondary-button" disabled={busy||!options.openReference} aria-label={`播放片段 ${page*20+index+1}`} onClick={()=>void run(()=>options.openReference!(reference,page*20+index))}><Play size={14}/>{learningTime(Number(range.startMs)||0)}–{learningTime(Number(range.endMs)||0)}</button>)}</div>{ranges.length>20&&<div><button type="button" disabled={page===0||busy} onClick={()=>setPage(value=>value-1)}>上一页</button><span>{page+1} / {Math.ceil(ranges.length/20)}</span><button type="button" disabled={(page+1)*20>=ranges.length||busy} onClick={()=>setPage(value=>value+1)}>下一页</button></div>}</>}
    {busy&&<p role="status">{preserved?"正在读取原文…":"正在核对视频来源…"}</p>}{error&&<p role="alert">{error}</p>}
  </NodeViewWrapper>;
}
const PreservedMarkdown=Node.create<LearningNoteOptions>({
  name:"preservedMarkdown",group:"block",atom:true,draggable:true,
  addAttributes(){return {archiveItemId:{default:"",rendered:false},label:{default:"保留原文",rendered:false}};},
  parseHTML(){return [];},
  renderHTML({node,HTMLAttributes}){return ["div",mergeAttributes(HTMLAttributes,{class:"knowledge-learning-atom","data-preserved-markdown":node.attrs.archiveItemId}),String(node.attrs.label||"保留原文")];},
  renderMarkdown(node){return `[${escapedLabel(node.attrs?.label)}](dccpreserved://${node.attrs?.archiveItemId})`;},
  addNodeView(){return ReactNodeViewRenderer(LearningAtom);},
});
const LearningReference=Node.create<LearningNoteOptions>({
  name:"learningReference",group:"block",atom:true,draggable:true,
  addAttributes(){return {courseId:{default:"",rendered:false},mediaId:{default:"",rendered:false},fingerprint:{default:"",rendered:false},ranges:{default:[],rendered:false},label:{default:"课程片段",rendered:false}};},
  parseHTML(){return [];},
  renderHTML({node,HTMLAttributes}){return ["div",mergeAttributes(HTMLAttributes,{class:"knowledge-learning-atom","data-learning-reference":node.attrs.courseId}),String(node.attrs.label||"课程片段")];},
  renderMarkdown(node){return referenceMarkdown(node.attrs as LearningReferenceData);},
  addNodeView(){return ReactNodeViewRenderer(LearningAtom);},
});
const isLearning=(node:DocumentNode)=>node.type.name==="preservedMarkdown"||node.type.name==="learningReference";
function counts(doc:DocumentNode):Map<string,number> {
  const values=new Map<string,number>();
  doc.descendants(node=>{if(isLearning(node)){const key=node.type.name==="preservedMarkdown"?`archive:${node.attrs.archiveItemId}`:JSON.stringify([node.attrs.courseId,node.attrs.mediaId,node.attrs.fingerprint,node.attrs.ranges]);values.set(key,(values.get(key)||0)+1);}});return values;
}
const LearningDeletionGuard=Extension.create<LearningNoteOptions>({
  name:"learningDeletionGuard",
  addProseMirrorPlugins(){return [new Plugin({filterTransaction:(transaction,state)=>{
    if(!transaction.docChanged||transaction.getMeta("preventUpdate"))return true;
    let candidate=false;
    transaction.mapping.maps.forEach((map,index)=>map.forEach((from,to)=>{if(to>from)transaction.docs[index].nodesBetween(from,to,node=>{if(isLearning(node))candidate=true;});}));
    if(!candidate)return true;
    const before=counts(state.doc),after=counts(transaction.doc);
    let removed=0;for(const [key,count]of before)removed+=Math.max(0,count-(after.get(key)||0));
    if(!removed)return true;
    const message=`删除 ${removed} 个学习引用或保留原文块？原文归档仍保留，可使用撤销恢复。`;
    return this.options.confirmDelete?this.options.confirmDelete(message):window.confirm(message);
  }})];},
});
export function learningNoteExtensions(options:LearningNoteOptions={}):Extensions {return [PreservedMarkdown.configure(options),LearningReference.configure(options),LearningDeletionGuard.configure(options)];}
export function hasLearningAtoms(content:JSONContent[]):boolean {
  const nodes=content.slice();while(nodes.length){const node=nodes.pop()!;if(node.type==="learningReference"||node.type==="preservedMarkdown")return true;nodes.push(...node.content||[]);}return false;
}
