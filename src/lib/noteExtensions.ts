import { Node, mergeAttributes } from "@tiptap/react";
import Image from "@tiptap/extension-image";
import {noteMapSvg} from "./noteMindMap";

export const NoteMindMapNode=Node.create({
  name:"mindMap",group:"block",atom:true,draggable:true,
  addAttributes(){return {id:{default:""},data:{default:null}};},
  parseHTML(){return [];},
  renderMarkdown(node){return `![思维导图](dccmap://${node.attrs?.id})`;},
  renderHTML({node}){
    let preview="";try{preview=`data:image/svg+xml;charset=utf-8,${encodeURIComponent(noteMapSvg(node.attrs.data))}`;}catch{/* Invalid persisted maps remain visible as an error, never active HTML. */}
    return ["div",{class:"knowledge-mind-map","data-note-map":node.attrs.id},["button",{type:"button"},"打开思维导图"],...(preview?[["img",{src:preview,alt:"思维导图预览"}]]:[["p",{},"导图格式无效"]])];
  },
});

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
