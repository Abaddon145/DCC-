import type {JSONContent} from "@tiptap/react";
import type {NoteMapNode,NoteMapData} from "./noteMindMap";

export interface StructureNode extends NoteMapNode { index:number; end:number; parent:string; children:StructureNode[] }
export function blockText(node:JSONContent):string {
  return (node.text||"")+(node.content||[]).map(blockText).join(node.type==="paragraph"||node.type==="heading"?"":" ");
}
export function structureRange(document:JSONContent,index:number):{start:number;end:number} {
  const blocks=document.content||[];const block=blocks[index];if(!block)throw new Error("该内容已更新，请重新选择节点");
  let end=index+1;if(block.type==="heading")while(end<blocks.length&&(blocks[end].type!=="heading"||Number(blocks[end].attrs?.level)>Number(block.attrs?.level)))end++;
  return {start:index,end};
}
export function noteStructure(document:JSONContent,title:string):StructureNode {
  const root:StructureNode={id:"root",topic:title||"未命名笔记",root:true,index:-1,end:(document.content||[]).length,parent:"",children:[]};
  const stack:{level:number;node:StructureNode}[]=[{level:0,node:root}];
  (document.content||[]).forEach((block,index)=>{
    const level=block.type==="heading"?Number(block.attrs?.level)||1:null;
    if(level!==null)while(stack.length>1&&stack.at(-1)!.level>=level)stack.pop();
    const label=blockText(block).trim()||String(block.attrs?.alt||block.attrs?.label||({image:"图片",table:"表格",mindMap:"独立思维导图",attachment:"附件",reference:"引用",paragraph:"空段落"} as Record<string,string>)[block.type||""]||block.type);
    const parent=stack.at(-1)!.node;const node:StructureNode={id:`b${index}`,topic:Array.from(label).slice(0,80).join("")+(Array.from(label).length>80?"…":""),index,end:structureRange(document,index).end,parent:parent.id,children:[]};parent.children.push(node);
    if(level!==null)stack.push({level,node});
  });return root;
}
export function findStructure(root:StructureNode,id:string):StructureNode|undefined { if(root.id===id)return root;for(const child of root.children){const result=findStructure(child,id);if(result)return result;} }
export function moveStructure(document:JSONContent,sourceIndex:number,targetIndex:number,zone:"inside"|"before"|"after"):JSONContent {
  const blocks=document.content||[];const source=structureRange(document,sourceIndex);const target=targetIndex<0?null:structureRange(document,targetIndex);
  if(targetIndex>=source.start&&targetIndex<source.end)throw new Error("不能移动到自身或后代节点");
  if(zone==="inside"&&target&&blocks[targetIndex].type!=="heading")throw new Error("只能将分支移入章节标题");
  let insertion=target?(zone==="before"?target.start:target.end):blocks.length;
  const root=noteStructure(document,"");const targetNode=target?findStructure(root,`b${targetIndex}`):root;
  const parent=zone==="inside"?targetNode:findStructure(root,targetNode?.parent||"root");
  const parentLevel=parent&&parent.index>=0?Number(blocks[parent.index].attrs?.level)||0:0;
  const moving=structuredClone(blocks.slice(source.start,source.end));
  if(moving[0].type==="heading"){
    const delta=parentLevel+1-Number(moving[0].attrs?.level);
    for(const block of moving)if(block.type==="heading"){const level=Number(block.attrs?.level)+delta;if(level<1||level>6)throw new Error("移动后的标题超过六级，请先调整章节层级");block.attrs={...block.attrs,level};}
  }
  const next=blocks.slice();next.splice(source.start,source.end-source.start);if(insertion>source.start)insertion-=source.end-source.start;next.splice(insertion,0,...moving);return {...document,content:next};
}
export function removeStructure(document:JSONContent,index:number):JSONContent {
  const {start,end}=structureRange(document,index);const next=(document.content||[]).slice();next.splice(start,end-start);return {...document,content:next.length?next:[{type:"paragraph"}]};
}
export function structureMap(root:StructureNode):NoteMapData {
  const visit=(node:StructureNode):NoteMapNode=>({id:node.id,topic:node.topic,root:node.root,children:node.children.map(visit)});return {nodeData:visit(root),direction:1};
}
