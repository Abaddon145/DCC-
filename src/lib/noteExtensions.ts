import { Node, mergeAttributes } from "@tiptap/react";
import Image from "@tiptap/extension-image";

export const ManagedNoteImage = Image.extend({
  parseHTML() { return [{ tag: 'img[src^="http://dcc-media.localhost/note/"]' }]; },
}).configure({ allowBase64: false });

export const NoteRemoteImageNode = Node.create({
  name:"remoteImage",group:"block",atom:true,
  addAttributes(){return {src:{default:""},alt:{default:"网络图片"},error:{default:"下载失败"}};},
  parseHTML(){return [];},
  renderMarkdown(node){return `![${String(node.attrs?.alt||"网络图片").replace(/[\\\[\]]/g,"\\$&")}](${String(node.attrs?.src||"").replace(/ /g,"%20")})`;},
  renderHTML({node}){return ["div",{class:"knowledge-remote-image","data-remote-image":node.attrs.src},["strong",{},String(node.attrs.alt||"网络图片")],["p",{},String(node.attrs.error||"图片未下载")],["button",{type:"button","data-retry-remote-image":node.attrs.src},"重试下载图片"],["a",{href:node.attrs.src},"查看原图片地址"]];}
});

export const NoteReferenceNode = Node.create({
  name: "reference", group: "block", atom: true, draggable: true,
  addAttributes() { return { kind: { default: "note" }, targetId: { default: "" }, label: { default: "引用" } }; },
  parseHTML() { return []; },
  renderMarkdown(node) { return `[${String(node.attrs?.label||"引用").replace(/[\\\[\]]/g,"\\$&")}](dccref://${node.attrs?.kind}/${node.attrs?.targetId})`; },
  renderHTML({ node, HTMLAttributes }) { return ["div", mergeAttributes(HTMLAttributes, { class: "knowledge-reference-card", "data-note-reference": node.attrs.targetId, "data-reference-kind": node.attrs.kind }), `↗ ${node.attrs.label}`]; },
});

export const NoteAttachmentNode = Node.create({
  name: "attachment", group: "block", atom: true,
  addAttributes() { return { attachmentId: { default: "" }, label: { default: "附件" } }; },
  parseHTML() { return []; },
  renderMarkdown(node) { return `[${String(node.attrs?.label||"附件").replace(/[\\\[\]]/g,"\\$&")}](http://dcc-media.localhost/note/${node.attrs?.attachmentId})`; },
  renderHTML({ node }) { return ["div", { class: "knowledge-reference-card", "data-note-attachment": node.attrs.attachmentId }, `📎 ${node.attrs.label}`]; },
});
