import type {JSONContent} from "@tiptap/react";
export interface NoteMapNode {id:string;topic:string;children?:NoteMapNode[];root?:boolean;expanded?:boolean;direction?:0|1}
export interface NoteMapData {nodeData:NoteMapNode;direction:1}
export function normalizeNoteMap(value:unknown):NoteMapData {
  const ids=new Set<string>();let count=0;
  function visit(raw:unknown,depth:number):NoteMapNode {
    if(!raw||typeof raw!=="object"||depth>32||++count>1000)throw new Error("思维导图最多支持 1000 个节点、32 层");
    const v=raw as Record<string,unknown>;
    if(typeof v.id!=="string"||! /^[a-zA-Z0-9_-]{1,80}$/.test(v.id)||ids.has(v.id)||typeof v.topic!=="string"||v.topic.length>1000)throw new Error("思维导图节点无效或重复");
    ids.add(v.id);
    if(v.children!==undefined&&!Array.isArray(v.children))throw new Error("思维导图子节点无效");
    return {id:v.id,topic:v.topic,expanded:v.expanded!==false,...(depth===0?{root:true}:{}),...(Array.isArray(v.children)?{children:v.children.map(child=>visit(child,depth+1))}:{})};
  }
  const input=value as {nodeData?:unknown}|null;
  return {nodeData:visit(input?.nodeData,0),direction:1};
}
export function mapFromHeadings(document:JSONContent,title:string):NoteMapData {
  const root:NoteMapNode={id:crypto.randomUUID(),topic:title.slice(0,1000)||"思维导图",root:true,children:[]};
  const parents:{level:number;node:NoteMapNode}[]=[{level:0,node:root}];
  const text=(node:JSONContent):string=>(node.text||"")+(node.content||[]).map(text).join("");
  for(const node of document.content||[]){
    if(node.type!=="heading")continue;
    const level=Number(node.attrs?.level)||1;
    while(parents.length>1&&parents[parents.length-1].level>=level)parents.pop();
    const child:NoteMapNode={id:crypto.randomUUID(),topic:text(node).slice(0,1000),children:[]};
    (parents[parents.length-1].node.children||=[]).push(child);parents.push({level,node:child});
  }
  return normalizeNoteMap({nodeData:root});
}
const escape=(text:string)=>text.replace(/[&<>"']/g,v=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&apos;"}[v]!));
export function noteMapSvg(value:unknown):string {
  const data=normalizeNoteMap(value);const rows:{node:NoteMapNode;depth:number;parent:number}[]=[];
  const add=(node:NoteMapNode,depth:number,parent:number)=>{const index=rows.length;rows.push({node,depth,parent});for(const child of node.children||[])add(child,depth+1,index);};add(data.nodeData,0,-1);
  const wrapped=rows.map(row=>{const chars=Array.from(row.node.topic);const lines:string[]=[];for(let i=0;i<chars.length;i+=14)lines.push(chars.slice(i,i+14).join(""));return lines.length?lines:[""];});
  let offset=20;const boxes=wrapped.map(lines=>{const box={y:offset,height:lines.length*20+20};offset+=box.height+16;return box;});
  const width=Math.max(360,...rows.map(row=>row.depth*220+260));const height=offset+16;
  const lines=rows.map((row,i)=>{if(row.parent<0)return "";const parent=rows[row.parent];return `<path d="M ${parent.depth*220+230} ${boxes[row.parent].y+boxes[row.parent].height/2} H ${row.depth*220+8} V ${boxes[i].y+boxes[i].height/2}" fill="none" stroke="#6C7B90"/>`;}).join("");
  const nodes=rows.map((row,i)=>`<g><rect x="${row.depth*220+16}" y="${boxes[i].y}" width="208" height="${boxes[i].height}" rx="6" fill="#202936" stroke="#64748B"/><text font-family="sans-serif" font-size="14" fill="#E4E8EF">${wrapped[i].map((line,j)=>`<tspan x="${row.depth*220+26}" y="${boxes[i].y+25+j*20}">${escape(line)}</tspan>`).join("")}</text></g>`).join("");
  const scale=Math.min(1,1600/width,1600/height);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${Math.max(1,Math.round(width*scale))}" height="${Math.max(1,Math.round(height*scale))}" viewBox="0 0 ${width} ${height}"><rect width="${width}" height="${height}" fill="#11151B"/>${lines}${nodes}</svg>`;
}
