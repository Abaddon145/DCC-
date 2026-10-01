export const noteShortcutDefaults:Record<string,string>={
  noteSibling:"Enter",noteIndent:"Tab",notePromote:"Shift+Tab",noteUndo:"Ctrl+Z",noteRedo:"Ctrl+Y",
  noteZoomIn:"Ctrl++",noteZoomOut:"Ctrl+-",noteFind:"Ctrl+F",noteGlobalFind:"Ctrl+Shift+F",noteScreenshot:"Ctrl+Shift+A",
  noteBold:"Ctrl+B",noteItalic:"Ctrl+I",noteUnderline:"Ctrl+U",noteStrike:"Ctrl+Enter",noteLink:"Ctrl+K",noteCode:"Alt+L",
  noteHeading1:"Alt+1",noteHeading2:"Alt+2",noteHeading3:"Alt+3",noteParagraph:"Alt+4",
  noteSiblingsHeading1:"Alt+Shift+1",noteSiblingsHeading2:"Alt+Shift+2",noteSiblingsHeading3:"Alt+Shift+3",noteSiblingsParagraph:"Alt+Shift+4",
};
export const noteShortcutLabels:Record<string,string>={noteSibling:"新建同级主题",noteIndent:"新建子主题",notePromote:"提升主题",noteUndo:"撤销",noteRedo:"重做",noteZoomIn:"放大",noteZoomOut:"缩小",noteFind:"文档内搜索",noteGlobalFind:"搜索全部笔记",noteScreenshot:"区域截屏",noteBold:"加粗",noteItalic:"斜体",noteUnderline:"下划线",noteStrike:"删除线",noteLink:"插入链接",noteCode:"行内代码",noteHeading1:"一级标题",noteHeading2:"二级标题",noteHeading3:"三级标题",noteParagraph:"正文",noteSiblingsHeading1:"同级一级标题",noteSiblingsHeading2:"同级二级标题",noteSiblingsHeading3:"同级三级标题",noteSiblingsParagraph:"同级正文"};
let preferences:Record<string,string>={};
export function setNoteShortcuts(value:Record<string,string>){preferences=value;}
export function activeNoteShortcutAction(event:KeyboardEvent){return noteShortcutAction(event,preferences);}

export function normalizeNoteShortcut(event:KeyboardEvent) {
  if(["Control","Shift","Alt","Meta"].includes(event.key))return "";
  const parts=[event.ctrlKey||event.metaKey?"Ctrl":"",event.altKey?"Alt":"",event.shiftKey?"Shift":""].filter(Boolean);
  let key=event.key.length===1?event.key.toUpperCase():event.key;
  if(/^Digit[1-4]$/.test(event.code))key=event.code.slice(-1);
  if(key==="+"||(event.code==="Equal"&&(event.ctrlKey||event.metaKey))){key="+";const i=parts.indexOf("Shift");if(i>=0)parts.splice(i,1);}
  return [...parts,key===" "?"Space":key].join("+");
}
export function noteShortcutAction(event:KeyboardEvent,shortcuts:Record<string,string>={}) {
  const value=normalizeNoteShortcut(event).toLowerCase();
  return Object.keys(noteShortcutDefaults).find(id=>(shortcuts[id]??noteShortcutDefaults[id]).toLowerCase()===value);
}
