import {noteMapSvg,type NoteMapData} from "./noteMindMap";
export async function noteMapPng(data:NoteMapData):Promise<number[]> {
  const svg=noteMapSvg(data);const source=URL.createObjectURL(new Blob([svg],{type:"image/svg+xml"}));
  try{
    const image=new Image();image.src=source;await image.decode();
    const canvas=document.createElement("canvas");canvas.width=image.naturalWidth;canvas.height=image.naturalHeight;
    if(!canvas.width||!canvas.height||canvas.width>1600||canvas.height>1600)throw new Error("导图图片尺寸无效");
    const context=canvas.getContext("2d");if(!context)throw new Error("无法创建导图图像");context.drawImage(image,0,0);
    const blob=await new Promise<Blob>((resolve,reject)=>canvas.toBlob(value=>value?resolve(value):reject(new Error("PNG 生成失败")),"image/png"));return Array.from(new Uint8Array(await blob.arrayBuffer()));
  }finally{URL.revokeObjectURL(source);}
}
/** Prints a static, backend-rendered document, never the application's navigation/editor UI. */
export async function printNoteDocument(html:string):Promise<void>{
  const frame=document.createElement("iframe");frame.title="笔记打印";frame.style.cssText="position:fixed;left:-10000px;top:0;width:900px;height:1000px;border:0";
  const ready=new Promise<void>((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error("打印文档打开超时，请重试")),15000);frame.onload=()=>{clearTimeout(timer);resolve();};frame.onerror=()=>{clearTimeout(timer);reject(new Error("无法打开打印文档"));};});
  frame.srcdoc=html;document.body.append(frame);
  try{await ready;const page=frame.contentWindow;if(!page)throw new Error("无法打开打印窗口");await Promise.all(Array.from(frame.contentDocument?.images||[]).map(image=>image.decode().catch(()=>{})));page.addEventListener("afterprint",()=>frame.remove(),{once:true});page.print();setTimeout(()=>frame.remove(),120000);}catch(error){frame.remove();throw error;}
}
