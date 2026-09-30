import {act,cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,describe,it,expect,vi} from "vitest";
import {Editor,EditorContent} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import {TableKit} from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import {NoteEditorTools} from "./NoteEditorTools";
import {ManagedNoteImage} from "../lib/noteExtensions";
const invoke=vi.hoisted(()=>vi.fn().mockResolvedValue(null));
vi.mock("@tauri-apps/api/core",()=>({invoke}));
Object.defineProperty(Range.prototype,"getClientRects",{configurable:true,value:()=>[]});
Object.defineProperty(Range.prototype,"getBoundingClientRect",{configurable:true,value:()=>({top:0,left:0,right:0,bottom:0,width:0,height:0})});
let editor:Editor;
afterEach(()=>{cleanup();editor?.destroy();vi.restoreAllMocks();invoke.mockReset().mockResolvedValue(null)});
function setup(disabled=false,table=false){
  editor=new Editor({extensions:[StarterKit,TableKit,TaskList,TaskItem.configure({nested:true})],content:table?'<table><tr><td><p>cell</p></td></tr></table>':'<h1>一级</h1><h3>三级</h3><p>learning</p>',editable:!disabled});
  const ai=vi.fn(),insert=vi.fn();render(<section className="knowledge-editor"><NoteEditorTools editor={editor} libraryId="library" disabled={disabled} onAi={ai} onInsert={insert}/><EditorContent editor={editor}/></section>);
  return{ai,insert};
}
describe("editing selection and contextual commands",()=>{
  it("cuts and pastes an image node without reducing it to empty text",async()=>{
    editor=new Editor({extensions:[StarterKit,ManagedNoteImage],content:{type:"doc",content:[{type:"image",attrs:{src:"http://dcc-media.localhost/note/11111111-1111-4111-8111-111111111111"}},{type:"paragraph"}]}});
    render(<section className="knowledge-editor"><NoteEditorTools editor={editor} libraryId="library" disabled={false} onAi={()=>{}} onInsert={()=>{}}/><EditorContent editor={editor}/></section>);
    act(()=>editor.commands.setNodeSelection(0));vi.spyOn(editor.view,"posAtCoords").mockReturnValue(null);
    let slice:unknown=null;invoke.mockImplementation(async(cmd:string,args?:Record<string,unknown>)=>{if(cmd==="write_note_clipboard_fragment"){slice=args?.slice;return null;}if(cmd==="read_note_clipboard_fragment")return slice;return null;});
    fireEvent.contextMenu(editor.view.dom);fireEvent.click(screen.getByRole("menuitem",{name:"剪切"}));await waitFor(()=>expect(editor.getJSON().content?.some(n=>n.type==="image")).toBe(false));
    fireEvent.contextMenu(editor.view.dom);fireEvent.click(screen.getByRole("menuitem",{name:"粘贴"}));await waitFor(()=>expect(editor.getJSON().content?.some(n=>n.type==="image")).toBe(true));
  });
  it("keeps a selected range when right-clicking inside it and copies only selected text",async()=>{
    setup();act(()=>editor.commands.setTextSelection({from:1,to:3}));vi.spyOn(editor.view,"posAtCoords").mockReturnValue({pos:2,inside:0});
    fireEvent.mouseDown(editor.view.dom,{button:2});fireEvent.contextMenu(editor.view.dom,{clientX:20,clientY:20});
    expect(editor.state.selection.from).toBe(1);expect(editor.state.selection.to).toBe(3);
    fireEvent.click(screen.getByRole("menuitem",{name:"复制"}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("write_note_clipboard_fragment",expect.objectContaining({libraryId:"library",slice:expect.any(Object)})));
    expect(editor.state.selection.to).toBe(3);
  });
  it("positions the caret outside a selection before applying a format",()=>{
    setup();act(()=>editor.commands.setTextSelection({from:1,to:3}));const position=editor.state.doc.content.size-2;
    vi.spyOn(editor.view,"posAtCoords").mockReturnValue({pos:position,inside:0});fireEvent.mouseDown(editor.view.dom,{button:2});fireEvent.contextMenu(editor.view.dom);
    expect(editor.state.selection.empty).toBe(true);expect(editor.state.selection.from).toBe(position);
    fireEvent.click(screen.getByRole("menuitem",{name:"二级标题"}));expect(editor.isActive("heading",{level:2})).toBe(true);
  });
  it("disables mutation in history or trash but permits copying",()=>{
    setup(true);act(()=>editor.commands.setTextSelection({from:1,to:3}));vi.spyOn(editor.view,"posAtCoords").mockReturnValue(null);fireEvent.contextMenu(editor.view.dom);
    expect(screen.getByRole("menuitem",{name:"剪切"})).toBeDisabled();expect(screen.getByRole("menuitem",{name:"粘贴"})).toBeDisabled();expect(screen.getByRole("menuitem",{name:"一级标题"})).toBeDisabled();expect(screen.getByRole("menuitem",{name:"复制"})).toBeEnabled();
  });
  it("offers row and column operations only inside a table",()=>{
    setup(false,true);act(()=>editor.commands.setTextSelection(4));vi.spyOn(editor.view,"posAtCoords").mockReturnValue(null);fireEvent.contextMenu(editor.view.dom);
    fireEvent.click(screen.getByRole("menuitem",{name:"在下方插入行"}));expect(editor.getJSON().content?.[0].content).toHaveLength(2);
  });
  it("shows a keyboard fallback when clipboard access fails without deleting the selection",async()=>{
    setup();invoke.mockRejectedValueOnce(new Error("denied"));act(()=>editor.commands.setTextSelection({from:1,to:3}));vi.spyOn(editor.view,"posAtCoords").mockReturnValue(null);fireEvent.contextMenu(editor.view.dom);fireEvent.click(screen.getByRole("menuitem",{name:"剪切"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("Ctrl+C / Ctrl+V");expect(editor.getText()).toContain("一级");
  });
});
