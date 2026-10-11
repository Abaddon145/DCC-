export type NoteView = "edit" | "read";
export interface NoteTab { id:string; title:string; pinned:boolean; view:NoteView; layout?:"document"|"outline"|"mindMap";color?:string|null; scroll:number; from:number; to:number }
export function mergeRestoredNoteTabs(restored:NoteTab[],opened:NoteTab[]):NoteTab[]{
  const activeIds=new Set(opened.map(tab=>tab.id));
  return [...restored.filter(tab=>!activeIds.has(tab.id)),...opened].slice(-50);
}
export function openNoteTab(tabs:NoteTab[],id:string,title:string):NoteTab[]{
  const existing=tabs.find(tab=>tab.id===id);if(existing)return tabs.map(tab=>tab.id===id?{...tab,title}:tab);
  if(tabs.length>=50)throw new Error("最多同时打开 50 篇笔记，请先关闭一些标签");
  return [...tabs,{id,title,pinned:false,view:"edit",scroll:0,from:1,to:1}];
}
export function sanitizeNoteTabs(value:unknown):NoteTab[]{
  if(!Array.isArray(value))return [];const seen=new Set<string>();return value.slice(0,50).flatMap(raw=>{
    if(!raw||typeof raw!=="object"||typeof raw.id!=="string"||!/^[\da-f-]{36}$/i.test(raw.id)||seen.has(raw.id))return [];
    seen.add(raw.id);return [{id:raw.id,title:typeof raw.title==="string"?raw.title.slice(0,300):"笔记",pinned:raw.pinned===true,color:typeof raw.color==="string"&&/^#[\da-f]{6}$/i.test(raw.color)?raw.color:null,layout:raw.layout==="outline"?"outline" as const:raw.layout==="mindMap"?"mindMap" as const:"document" as const,view:raw.view==="read"?"read" as const:"edit" as const,scroll:Math.max(0,Math.min(1e7,Number(raw.scroll)||0)),from:Math.max(1,Math.min(1e7,Number(raw.from)||1)),to:Math.max(1,Math.min(1e7,Number(raw.to)||1))}];
  });
}
export interface NoteAppearance { background:string; text:string; accent:string; fontSize:number; lineHeight:number; contentWidth:number; navigationWidth:number; listWidth:number; codeTheme:"dark"|"muted" }
export const defaultNoteAppearance:NoteAppearance={background:"#11151B",text:"#DCE1E8",accent:"#D99A42",fontSize:16,lineHeight:1.85,contentWidth:860,navigationWidth:200,listWidth:280,codeTheme:"dark"};
export const noteAppearancePresets:Record<string,NoteAppearance>={
  "石墨":defaultNoteAppearance,
  "夜读":{...defaultNoteAppearance,background:"#0D1420",text:"#D5DFED",accent:"#8AAED1"},
  "浅色":{...defaultNoteAppearance,background:"#F2F4F7",text:"#242A33",accent:"#825619",codeTheme:"muted"},
};
export function noteColorContrast(a:string,b:string):number {
  const luminance=(color:string)=>{const rgb=[1,3,5].map(offset=>parseInt(color.slice(offset,offset+2),16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;};
  const first=luminance(a),second=luminance(b);return (Math.max(first,second)+.05)/(Math.min(first,second)+.05);
}
export function validateNoteAppearance(value:NoteAppearance):void {
  if(noteColorContrast(value.text,value.background)<4.5)throw new Error("正文与背景对比度不足，请选择更清晰的颜色");
  if(noteColorContrast(value.accent,value.background)<3)throw new Error("强调色与背景太接近，请调整强调色");
}
export function normalizeNoteAppearance(value:unknown):NoteAppearance{
  const raw=(value&&typeof value==="object"?value:{}) as Partial<NoteAppearance>;
  const color=(v:unknown,fallback:string)=>typeof v==="string"&&/^#[\da-f]{6}$/i.test(v)?v:fallback;
  const number=(v:unknown,fallback:number,min:number,max:number)=>typeof v==="number"&&Number.isFinite(v)?Math.max(min,Math.min(max,v)):fallback;
  return {background:color(raw.background,defaultNoteAppearance.background),text:color(raw.text,defaultNoteAppearance.text),accent:color(raw.accent,defaultNoteAppearance.accent),fontSize:number(raw.fontSize,16,12,24),lineHeight:number(raw.lineHeight,1.85,1.3,2.5),contentWidth:number(raw.contentWidth,860,540,1400),navigationWidth:number(raw.navigationWidth,200,160,340),listWidth:number(raw.listWidth,280,220,440),codeTheme:raw.codeTheme==="muted"?"muted":"dark"};
}
