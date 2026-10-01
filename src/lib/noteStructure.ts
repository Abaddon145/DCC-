import type {JSONContent} from "@tiptap/react";
import type {NoteMapNode,NoteMapData} from "./noteMindMap";

export interface StructureNode extends NoteMapNode { index:number; end:number; parent:string; children:StructureNode[]; text?:string;images?:string[]; branchColor?:string }
export function blockText(node:JSONContent):string {
  return (node.text||"")+(node.content||[]).map(blockText).join(node.type==="paragraph"||node.type==="heading"?"":" ");
}
// Hierarchy is independent of heading typography. Legacy documents are projected
// without mutation; the first structural edit records their existing relationships.
export function structureDepths(document:JSONContent):number[] {
  const headings:number[]=[];
  const structured=(document.content||[]).some(block=>block.attrs?.topicDepth!=null);let previous=0;
  return (document.content||[]).map(block=>{
    const explicit=block.attrs?.topicDepth;
    if(explicit!==undefined&&explicit!==null){
      if(!Number.isInteger(explicit)||explicit<0||explicit>32)throw Error("主题层级无效或超过安全上限 32 层");
      previous=explicit;return explicit;
    }
    if(structured)return previous;
    if(block.type==="heading"){
      const level=Number(block.attrs?.level)||1;
      while(headings.length&&headings.at(-1)!>=level)headings.pop();
      const depth=headings.length;headings.push(level);return depth;
    }
    return headings.length;
  });
}
export function normalizeStructure(document:JSONContent):JSONContent {
  const depths=structureDepths(document);
  const ids=new Set<string>();
  return {...document,content:(document.content||[]).map((block,i)=>{const candidate=block.attrs?.topicId;const topicId=typeof candidate==="string"&&/^[a-f0-9-]{36}$/i.test(candidate)&&!ids.has(candidate)?candidate:crypto.randomUUID();ids.add(topicId);return {...block,attrs:{...block.attrs,topicDepth:depths[i],topicId}};})};
}
export function structureRange(document:JSONContent,index:number):{start:number;end:number} {
  const blocks=document.content||[];const block=blocks[index];if(!block)throw new Error("该内容已更新，请重新选择节点");
  const depths=structureDepths(document);let end=index+1;while(end<blocks.length&&depths[end]>depths[index])end++;
  return {start:index,end};
}
export function noteStructure(document:JSONContent,title:string):StructureNode {
  const root:StructureNode={id:"root",topic:title||"未命名笔记",root:true,index:-1,end:(document.content||[]).length,parent:"",children:[]};
  const depths=structureDepths(document);const stack:{level:number;node:StructureNode}[]=[{level:-1,node:root}];
  (document.content||[]).forEach((block,index)=>{
    const level=depths[index];
    while(stack.length>1&&stack.at(-1)!.level>=level)stack.pop()!.node.end=index;
    const label=blockText(block).trim()||String(block.attrs?.alt||block.attrs?.label||({image:"图片",table:"表格",mindMap:"独立思维导图",attachment:"附件",reference:"引用",paragraph:"空段落"} as Record<string,string>)[block.type||""]||block.type);
    const images:string[]=[];const collect=(value:JSONContent)=>{if(value.type==="image"&&/^http:\/\/dcc-media\.localhost\/note\/[a-f0-9-]{36}$/i.test(value.attrs?.src||""))images.push(value.attrs!.src);value.content?.forEach(collect);};collect(block);
    const parent=stack.at(-1)!.node;const node:StructureNode={id:block.attrs?.topicId||`b${index}`,text:label,topic:Array.from(label).slice(0,80).join("")+(Array.from(label).length>80?"…":""),index,end:root.end,parent:parent.id,children:[],images,...(/^#[a-f0-9]{6}$/i.test(block.attrs?.topicColor||"")?{branchColor:block.attrs!.topicColor}:{})};parent.children.push(node);
    stack.push({level,node});
  });return root;
}
export function findStructure(root:StructureNode,id:string):StructureNode|undefined { if(root.id===id)return root;for(const child of root.children){const result=findStructure(child,id);if(result)return result;} }
export function structureAt(root:StructureNode,index:number):StructureNode|undefined {if(root.index===index)return root;for(const child of root.children){const value=structureAt(child,index);if(value)return value;}}
export function moveStructure(document:JSONContent,sourceIndex:number,targetIndex:number,zone:"inside"|"before"|"after"):JSONContent {
  document=normalizeStructure(document);const blocks=document.content||[];const source=structureRange(document,sourceIndex);const target=targetIndex<0?null:structureRange(document,targetIndex);
  if(targetIndex>=source.start&&targetIndex<source.end)throw new Error("不能移动到自身或后代节点");
  let insertion=target?(zone==="before"?target.start:target.end):blocks.length;
  const root=noteStructure(document,"");const targetNode=target?structureAt(root,targetIndex):root;
  const parent=zone==="inside"?targetNode:findStructure(root,targetNode?.parent||"root");
  const parentLevel=parent&&parent.index>=0?Number(blocks[parent.index].attrs?.topicDepth):-1;
  const moving=structuredClone(blocks.slice(source.start,source.end));
  const delta=parentLevel+1-Number(moving[0].attrs?.topicDepth);
  for(const block of moving){const depth=Number(block.attrs?.topicDepth)+delta;if(depth<0||depth>32)throw new Error("移动后的主题超过安全上限 32 层");block.attrs={...block.attrs,topicDepth:depth};}
  const next=blocks.slice();next.splice(source.start,source.end-source.start);if(insertion>source.start)insertion-=source.end-source.start;next.splice(insertion,0,...moving);return {...document,content:next};
}
export function removeStructure(document:JSONContent,index:number):JSONContent {
  document=normalizeStructure(document);
  const {start,end}=structureRange(document,index);const next=(document.content||[]).slice();next.splice(start,end-start);return {...document,content:next.length?next:[{type:"paragraph"}]};
}
export function structureMap(root:StructureNode):NoteMapData {
  const visit=(node:StructureNode):NoteMapNode=>({id:node.id,topic:node.topic+(node.images&&node.images.length>1?` · ${node.images.length} 张图片`:""),root:node.root,...(node.branchColor?{branchColor:node.branchColor}:{}),...(node.images?.length?{image:{url:node.images[0],width:160,height:100,fit:"contain" as const}}:{}),children:node.children.map(visit)});return {nodeData:visit(root),direction:1};
}

export function insertStructureSibling(document:JSONContent,index:number):{document:JSONContent;index:number} {
  document=normalizeStructure(document);
  const blocks=(document.content||[]).slice();
  const position=index<0?blocks.length:structureRange(document,index).end;
  const current=blocks[index];
  blocks.splice(position,0,current?.type==="heading"
    ?{type:"heading",attrs:{topicId:crypto.randomUUID(),level:current.attrs?.level||1,topicDepth:current.attrs?.topicDepth||0},content:[{type:"text",text:"新主题"}]}
    :{type:"paragraph",attrs:{topicId:crypto.randomUUID(),topicDepth:current?.attrs?.topicDepth||0},content:[{type:"text",text:"新内容"}]});
  return {document:{...document,content:blocks},index:position};
}
export function insertStructureChild(document:JSONContent,index:number):{document:JSONContent;index:number} {
  document=normalizeStructure(document);const blocks=(document.content||[]).slice();
  const depth=index<0?0:Number(blocks[index]?.attrs?.topicDepth)+1;
  if(!Number.isInteger(depth)||depth>32)throw Error("主题超过安全上限 32 层，请拆分笔记");
  const position=index<0?blocks.length:structureRange(document,index).end;
  blocks.splice(position,0,{type:"paragraph",attrs:{topicId:crypto.randomUUID(),topicDepth:depth},content:[{type:"text",text:"新主题"}]});
  return {document:{...document,content:blocks},index:position};
}

export function indentStructure(document:JSONContent,index:number,promote=false):JSONContent {
  const root=noteStructure(document,"");const node=structureAt(root,index);
  if(!node)throw Error("请选择内容主题");
  const parent=findStructure(root,node.parent)!;
  if(promote){if(parent.index<0)throw Error("已经位于根级");return moveStructure(document,index,parent.index,"after");}
  const previous=parent.children[parent.children.findIndex(n=>n.id===node.id)-1];
  if(!previous)throw Error("当前主题前没有同级主题");
  return moveStructure(document,index,previous.index,"inside");
}

export function formatStructure(document:JSONContent,index:number,level:number,siblings=false):JSONContent {
  document=normalizeStructure(document);
  if(![0,1,2,3].includes(level))throw Error("无效的标题级别");
  const root=noteStructure(document,"");const node=structureAt(root,index);
  if(!node)throw Error("请选择内容主题");
  const indices=new Set(siblings?findStructure(root,node.parent)!.children.map(n=>n.index):[index]);
  const content=(document.content||[]).map((block,i)=>{
    if(!indices.has(i)||!["heading","paragraph"].includes(block.type||""))return block;
    const attrs={...block.attrs};delete attrs.level;
    return {...block,type:level?"heading":"paragraph",attrs:level?{...attrs,level}:attrs};
  });return {...document,content};
}
