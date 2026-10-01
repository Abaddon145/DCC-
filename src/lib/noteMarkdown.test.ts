import {describe,expect,it} from "vitest";
import {parseNoteMarkdown,serializeNoteMarkdown} from "./noteMarkdown";

describe("portable note Markdown",()=>{
  it("exports hierarchy independently of heading style with image links retained",()=>{
    const result=serializeNoteMarkdown({type:"doc",content:[{type:"heading",attrs:{level:1,topicDepth:0},content:[{type:"text",text:"主题"}]},{type:"paragraph",attrs:{topicDepth:1},content:[{type:"text",text:"子主题"}]},{type:"image",attrs:{topicDepth:2,src:"http://dcc-media.localhost/note/image",alt:"示例"}}]});
    expect(result).toContain("- # 主题");expect(result).toContain("    - 子主题");expect(result).toContain("        - ![示例]");
  });
  it("retains retryable remote images in Markdown without embedding HTML",()=>{
    const document={type:"doc",content:[{type:"remoteImage",attrs:{src:"https://example.com/image.png",alt:"模型示例",error:"下载失败"}}]};
    expect(serializeNoteMarkdown(document)).toContain("![模型示例](https://example.com/image.png)");
    const nested=parseNoteMarkdown("# 教程\n\n- 步骤\n  - 操作\n\n    ![示例](https://example.com/image.png)");
    expect(JSON.stringify(nested)).toContain("https://example.com/image.png");
  });
  it("parses headings, tasks, code and relative images without resolving files",()=>{
    const document=parseNoteMarkdown("## 材质学习\n\n- [ ] 练习\n\n```hlsl\nfloat x = 1;\n```\n\n![预览](assets/preview.png)");
    const json=JSON.stringify(document);expect(json).toContain('"heading"');expect(json).toContain('"taskList"');expect(json).toContain('"codeBlock"');expect(json).toContain("assets/preview.png");
    const result=serializeNoteMarkdown(document);expect(result).toContain("## 材质学习");expect(result).toContain("assets/preview.png");
  });
  it("exports application references and managed attachments explicitly",()=>{
    const markdown=serializeNoteMarkdown({type:"doc",content:[{type:"reference",attrs:{kind:"asset",targetId:"asset-id",label:"模型"}},{type:"attachment",attrs:{attachmentId:"file-id",label:"PDF 资料"}}]});
    expect(markdown).toContain("dccref://asset/asset-id");expect(markdown).toContain("http://dcc-media.localhost/note/file-id");expect(markdown).toContain("PDF 资料");
  });
  it("rejects embedded HTML resources before DOM parsing but keeps fenced examples",()=>{
    expect(()=>parseNoteMarkdown('<img src="https://example.com/tracker.png">')).toThrow("HTML");
    expect(()=>parseNoteMarkdown('```html\n<img src="example.png">\n```')).not.toThrow();
  });
});
