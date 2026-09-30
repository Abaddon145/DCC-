import { MarkdownManager } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { TableKit } from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Image from "@tiptap/extension-image";
import { NoteReferenceNode, NoteAttachmentNode } from "./noteExtensions";
import type { JSONContent } from "@tiptap/react";

const manager = new MarkdownManager({ extensions: [StarterKit,TableKit,TaskList,TaskItem,Image,NoteReferenceNode,NoteAttachmentNode] });
export function parseNoteMarkdown(text:string):JSONContent {
  // Check lexer tokens before any DOM parsing; HTML inside fenced code remains ordinary code.
  let unsafe=false;
  manager.instance.walkTokens(manager.instance.lexer(text),token=>{if(token.type==="html"&&!/^(?:<br\s*\/?>|\s)+$/i.test(token.raw))unsafe=true;});
  if(unsafe)throw new Error("Markdown 含嵌入式 HTML，请转成标准 Markdown 后导入；代码块中的 HTML 不受影响。");
  return manager.parse(text);
}
export function serializeNoteMarkdown(document:JSONContent):string { return manager.serialize(document); }
