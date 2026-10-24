import {act,cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {Editor,EditorContent} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import {afterEach,expect,it,vi} from "vitest";
import {learningNoteExtensions,type LearningNoteOptions} from "./learningNoteNodes";

const item="59f222de-82c9-4e8e-8352-17142811caa7";
const reference={courseId:"596d257d-3e5c-46c4-a464-187cf180c6bd",mediaId:"b01c829f-de85-4671-8464-5e7db03edf8a",fingerprint:"a".repeat(64),ranges:[{startMs:1000,endMs:3000},{startMs:5000,endMs:6000}],label:"材质课 <script>"};
const block={type:"preservedMarkdown",attrs:{archiveItemId:item,label:"原文 <script>"}};
const document={type:"doc",content:[{type:"learningReference",attrs:reference},block,{type:"paragraph",content:[{type:"text",text:"可编辑心得"}]}]};
const editors:Editor[]=[];
function editor(options:LearningNoteOptions={}) {const value=new Editor({extensions:[StarterKit,...learningNoteExtensions(options)],content:document});editors.push(value);return value;}
afterEach(()=>{cleanup();editors.splice(0).forEach(value=>value.destroy());vi.restoreAllMocks();});

it("retains both atom payloads and renders labels as inert text",()=>{
  let value:Editor|undefined;
  expect(()=>{value=editor();}).not.toThrow();
  expect(value!.getJSON().content![0]).toEqual({type:"learningReference",attrs:reference});
  expect(value!.getJSON().content![1]).toEqual(block);
  expect(value!.getHTML()).not.toContain("<script>");
});

it("confirms actual deletion, permits moves and restores a deleted block with undo",()=>{
  let allow=false;let prompts=0;
  const value=editor({confirmDelete:()=>{prompts++;return allow;}});
  value.commands.setNodeSelection(0);
  value.commands.deleteSelection();
  expect(value.getJSON().content![0].type).toBe("learningReference");
  expect(prompts).toBe(1);
  const node=value.state.doc.firstChild!;
  value.view.dispatch(value.state.tr.delete(0,node.nodeSize).insert(value.state.doc.content.size-node.nodeSize,node));
  expect(prompts).toBe(1);
  expect(value.getJSON().content!.filter(block=>block.content?.length||block.type!=="paragraph").map(block=>block.type)).toEqual(["preservedMarkdown","paragraph","learningReference"]);
  allow=true;value.commands.setNodeSelection(0);value.commands.deleteSelection();
  expect(value.getJSON().content!.some(block=>block.type==="preservedMarkdown")).toBe(false);
  value.commands.undo();
  expect(value.getJSON().content!.some(block=>block.type==="preservedMarkdown")).toBe(true);
});

it("does not turn pasted foreign HTML into a trusted archive or course reference",()=>{
  const value=editor();
  value.commands.setContent('<div data-preserved-markdown="'+item+'">foreign</div><div data-learning-reference="'+reference.courseId+'">untrusted</div>',{emitUpdate:false});
  expect(value.getJSON().content!.every(block=>!["learningReference","preservedMarkdown"].includes(block.type!))).toBe(true);
});

it("loads raw Markdown on demand, announces failures and retries without executing HTML",async()=>{
  let attempts=0;
  const value=editor({readPreserved:async()=>{attempts++;if(attempts===1)throw Error("归档暂时不可读");return "---\r\n<script>notExecutable()</script>\r\n";}});
  render(<EditorContent editor={value}/>);
  expect(screen.queryByRole("textbox",{name:"保留的 Markdown 原文"})).toBeNull();
  fireEvent.click(await screen.findByRole("button",{name:"查看保留原文"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("归档暂时不可读");
  fireEvent.click(screen.getByRole("button",{name:"重试读取原文"}));
  const raw=await screen.findByRole("textbox",{name:"保留的 Markdown 原文"});
  expect(raw).toHaveValue("---\n<script>notExecutable()</script>\n");
  expect(raw).toHaveAttribute("readonly");
  expect(raw.closest(".knowledge-learning-atom")!.querySelector("script")).toBeNull();
});

it("opens the selected non-contiguous range and reports navigation errors",async()=>{
  let selected:number|undefined;
  const value=editor({openReference:async(data,index)=>{expect(data).toEqual(reference);selected=index;throw Error("视频原件已变化");}});
  render(<EditorContent editor={value}/>);
  fireEvent.click(screen.getByRole("button",{name:"播放片段 2"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("视频原件已变化");
  expect(selected).toBe(1);
});

it("ignores late raw responses after switching the archive identity",async()=>{
  let finish!:(value:string)=>void;
  const value=editor({readPreserved:()=>new Promise(resolve=>{finish=resolve;})});
  render(<EditorContent editor={value}/>);
  fireEvent.click(await screen.findByRole("button",{name:"查看保留原文"}));
  await waitFor(()=>expect(screen.getByRole("status")).toHaveTextContent("正在读取原文"));
  act(()=>value.commands.setContent({type:"doc",content:[{...block,attrs:{archiveItemId:reference.mediaId,label:"另一个原文"}}]},{emitUpdate:false}));
  await act(async()=>finish("旧库响应不应显示"));
  expect(screen.queryByDisplayValue("旧库响应不应显示")).toBeNull();
});
