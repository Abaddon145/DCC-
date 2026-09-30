import {describe,it,expect} from "vitest";
import {normalizeNoteMap,mapFromHeadings,noteMapSvg} from "./noteMindMap";
describe("embedded note mind maps",()=>{
  it("creates hierarchical headings without changing prose",()=>{const doc={type:"doc",content:[{type:"heading",attrs:{level:1},content:[{type:"text",text:"材质"}]},{type:"heading",attrs:{level:2},content:[{type:"text",text:"参数"}]}]};const before=JSON.stringify(doc);const map=mapFromHeadings(doc,"学习");expect(map.nodeData.children?.[0].children?.[0].topic).toBe("参数");expect(JSON.stringify(doc)).toBe(before);});
  it("rejects repeated ids and strips active content",()=>{expect(()=>normalizeNoteMap({nodeData:{id:"a",topic:"x",children:[{id:"a",topic:"y"}]}})).toThrow();expect(normalizeNoteMap({nodeData:{id:"a",topic:"x",hyperLink:"javascript:bad",image:{url:"https://bad"}}}).nodeData).not.toHaveProperty("hyperLink");});
  it("escapes text in offline previews",()=>{const svg=noteMapSvg({nodeData:{id:"a",topic:"<script>alert(1)"}});expect(svg).not.toContain("<script>");expect(svg).toContain("&lt;script&gt;");});
});
