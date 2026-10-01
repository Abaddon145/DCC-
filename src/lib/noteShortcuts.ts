import {Extension,type Editor} from "@tiptap/react";
import {Plugin} from "@tiptap/pm/state";
import {formatStructure,insertStructureChild,insertStructureSibling,indentStructure} from "./noteStructure";

import {activeNoteShortcutAction,noteShortcutAction} from "./noteShortcutKeys";
export {noteShortcutDefaults,noteShortcutLabels,setNoteShortcuts,noteShortcutAction,activeNoteShortcutAction} from "./noteShortcutKeys";

export function applyNoteFormat(editor:Editor,id:string) {
  const commands:Record<string,()=>boolean>={noteBold:()=>editor.commands.toggleBold(),noteItalic:()=>editor.commands.toggleItalic(),noteUnderline:()=>editor.commands.toggleUnderline(),noteStrike:()=>editor.commands.toggleStrike(),noteCode:()=>editor.commands.toggleCode()};
  if(commands[id])return commands[id]();
  const heading=/^note(Siblings)?(Heading([123])|Paragraph)$/.exec(id);
  if(!heading)return false;
  const level=Number(heading[3]||0);
  if(!heading[1])return level?editor.commands.setHeading({level:level as 1|2|3}):editor.commands.setParagraph();
  const index=editor.state.selection.$from.index(0);
  const next=formatStructure(editor.getJSON(),index,level,true);
  editor.view.dispatch(editor.state.tr.replaceWith(0,editor.state.doc.content.size,editor.schema.nodeFromJSON(next).content));
  return true;
}

/** Lists, tables and code retain their native Enter/Tab. */
export const NoteFormattingShortcuts=Extension.create({
  name:"noteFormattingShortcuts",priority:2000,
  addProseMirrorPlugins(){
    return [new Plugin({props:{handleKeyDown:(_view,event)=>{
      if(event.isComposing)return false;
      if((event.ctrlKey||event.metaKey)&&!event.altKey&&!event.shiftKey&&event.key.toLowerCase()==="e"){
        this.editor.view.dom.dispatchEvent(new CustomEvent("note-switch-view",{bubbles:true}));return true;
      }
      const id=activeNoteShortcutAction(event);
      if(this.editor.isEditable&&["noteIndent","noteSibling","notePromote"].includes(id||"")){
        const {$from}=this.editor.state.selection;
        if($from.depth!==1||!["paragraph","heading"].includes($from.parent.type.name)||this.editor.view.dom.getAttribute("aria-label")==="节点完整内容")return false;
        const index=$from.index(0),document=this.editor.getJSON();
        try{
          const result=id==="notePromote"?{document:indentStructure(document,index,true),index}:id==="noteIndent"?insertStructureChild(document,index):insertStructureSibling(document,index);
          if(id==="notePromote"){const sourceId=document.content?.[index].attrs?.topicId;const destination=result.document.content?.findIndex(block=>block.attrs?.topicId===sourceId)??index;if(destination>=0)result.index=destination;}
          const value=this.editor.schema.nodeFromJSON(result.document);let position=1;for(let i=0;i<result.index;i++)position+=value.child(i).nodeSize;
          this.editor.commands.setContent(result.document);
          this.editor.commands.setTextSelection({from:position,to:position+value.child(result.index).content.size});
        }catch(error){this.editor.view.dom.dispatchEvent(new CustomEvent("note-structure-error",{bubbles:true,detail:String(error)}));}
        return true;
      }
      if(this.editor.isEditable&&id==="noteUndo")return this.editor.commands.undo();
      if(this.editor.isEditable&&id==="noteRedo")return this.editor.commands.redo();
      const original=noteShortcutAction(event);
      if(!id&&original&&/^note(Bold|Italic|Underline|Strike|Code|Heading|Paragraph|Siblings|Undo|Redo)/.test(original))return true;
      if(!id||!/^note(Bold|Italic|Underline|Strike|Code|Heading|Paragraph|Siblings)/.test(id))return false;
      return this.editor.isEditable&&applyNoteFormat(this.editor,id);
    }}})];
  },
});
