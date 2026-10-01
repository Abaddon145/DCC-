import {describe,it,expect} from "vitest";
import {noteStructure,moveStructure,removeStructure} from "./noteStructure";
import type {JSONContent} from "@tiptap/react";
const heading=(text:string,level=1):JSONContent=>({type:"heading",attrs:{level},content:[{type:"text",text}]});
const rich:JSONContent={type:"bulletList",content:[{type:"listItem",content:[{type:"paragraph",content:[{type:"text",text:"不可丢失",marks:[{type:"bold"}]}]}]}]};
const blocks:JSONContent[]=[heading("A"),rich,{type:"image",attrs:{src:"managed"}},heading("B"),heading("子节",2),{type:"mindMap",attrs:{id:"old",data:{}}},{type:"table",content:[]}];
describe("canonical note structure",()=>{
  it("projects every rich block without altering canonical content",()=>{const doc={type:"doc",content:blocks};const before=JSON.stringify(doc);const root=noteStructure(doc,"笔记");expect(root.children).toHaveLength(2);expect(root.children[0].children.map(n=>n.index)).toEqual([1,2]);expect(root.children[1].children[0].children.map(n=>n.index)).toEqual([5,6]);expect(JSON.stringify(doc)).toBe(before);});
  it("moves whole sections, adjusts heading hierarchy and preserves attachments and formatting",()=>{const doc={type:"doc",content:blocks};const result=moveStructure(doc,0,4,"inside");expect(result.content?.[4].attrs?.level).toBe(3);expect(result.content?.slice(-2)).toEqual([rich,blocks[2]]);expect(()=>moveStructure(doc,3,4,"inside")).toThrow(/后代/);});
  it("deletes exactly the branch and supports restoring the original snapshot",()=>{const doc={type:"doc",content:blocks};expect(removeStructure(doc,3).content).toEqual(blocks.slice(0,3));expect(doc.content).toHaveLength(7);});
  it("rejects hierarchy overflow before mutating anything",()=>{const doc={type:"doc",content:[heading("A",1),heading("child",6),heading("B",1),heading("target",6)]};expect(()=>moveStructure(doc,0,3,"inside")).toThrow(/六级/);expect(doc.content[0].attrs?.level).toBe(1);});
});
