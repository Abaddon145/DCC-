import {describe,it,expect} from "vitest";
import {noteStructure,moveStructure,removeStructure,insertStructureSibling,indentStructure,formatStructure} from "./noteStructure";
import type {JSONContent} from "@tiptap/react";
const heading=(text:string,level=1):JSONContent=>({type:"heading",attrs:{level},content:[{type:"text",text}]});
const rich:JSONContent={type:"bulletList",content:[{type:"listItem",content:[{type:"paragraph",content:[{type:"text",text:"不可丢失",marks:[{type:"bold"}]}]}]}]};
const blocks:JSONContent[]=[heading("A"),rich,{type:"image",attrs:{src:"managed"}},heading("B"),heading("子节",2),{type:"mindMap",attrs:{id:"old",data:{}}},{type:"table",content:[]}];
describe("canonical note structure",()=>{
  it("inserts after the whole branch, not inside its children",()=>{const result=insertStructureSibling({type:"doc",content:blocks},3);expect(result.index).toBe(7);expect(result.document.content?.[7].attrs?.level).toBe(1);});
  it("indents existing headings and leaves the original immutable",()=>{const doc={type:"doc",content:[heading("A"),heading("B")]};expect(indentStructure(doc,1).content?.[1].attrs?.level).toBe(2);expect(doc.content[1].attrs?.level).toBe(1);expect(()=>indentStructure(doc,0)).toThrow(/前一个标题/);});
  it("formats only siblings under the same parent, preserving rich blocks",()=>{const result=formatStructure({type:"doc",content:blocks},0,2,true);expect(result.content?.[0].attrs?.level).toBe(2);expect(result.content?.[3].attrs?.level).toBe(2);expect(result.content?.[4]).toEqual(blocks[4]);expect(result.content?.[1]).toEqual(rich);});
  it("projects every rich block without altering canonical content",()=>{const doc={type:"doc",content:blocks};const before=JSON.stringify(doc);const root=noteStructure(doc,"笔记");expect(root.children).toHaveLength(2);expect(root.children[0].children.map(n=>n.index)).toEqual([1,2]);expect(root.children[1].children[0].children.map(n=>n.index)).toEqual([5,6]);expect(JSON.stringify(doc)).toBe(before);});
  it("moves whole sections, adjusts heading hierarchy and preserves attachments and formatting",()=>{const doc={type:"doc",content:blocks};const result=moveStructure(doc,0,4,"inside");expect(result.content?.[4].attrs?.level).toBe(3);expect(result.content?.slice(-2)).toEqual([rich,blocks[2]]);expect(()=>moveStructure(doc,3,4,"inside")).toThrow(/后代/);});
  it("deletes exactly the branch and supports restoring the original snapshot",()=>{const doc={type:"doc",content:blocks};expect(removeStructure(doc,3).content).toEqual(blocks.slice(0,3));expect(doc.content).toHaveLength(7);});
  it("rejects hierarchy overflow before mutating anything",()=>{const doc={type:"doc",content:[heading("A",1),heading("child",6),heading("B",1),heading("target",6)]};expect(()=>moveStructure(doc,0,3,"inside")).toThrow(/六级/);expect(doc.content[0].attrs?.level).toBe(1);});
});
