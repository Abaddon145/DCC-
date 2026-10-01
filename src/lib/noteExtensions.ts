import { Extension, Node, mergeAttributes } from "@tiptap/react";
import Image from "@tiptap/extension-image";
import {noteMapSvg} from "./noteMindMap";
import {Plugin} from "@tiptap/pm/state";
import {normalizeStructure} from "./noteStructure";

export const NoteTopicHierarchy=Extension.create({
  name:"noteTopicHierarchy",
  addProseMirrorPlugins(){return [new Plugin({appendTransaction:(transactions,_old,state)=>{
    if(!transactions.some(transaction=>transaction.docChanged))return null;
    const normalized=normalizeStructure(state.doc.toJSON());const transaction=state.tr;let index=0;
    state.doc.forEach((block,position)=>{const attrs=normalized.content![index++].attrs!;if(block.attrs.topicDepth!==attrs.topicDepth||block.attrs.topicId!==attrs.topicId)transaction.setNodeMarkup(position,undefined,{...block.attrs,topicDepth:attrs.topicDepth,topicId:attrs.topicId});});
    return transaction.steps.length?transaction:null;
  }})];},
  addGlobalAttributes(){return [{types:["doc"],attributes:{mapDirection:{default:2},mapTheme:{default:"graphite"}}},{types:["paragraph","heading","bulletList","orderedList","taskList","blockquote","codeBlock","horizontalRule","table","image","remoteImage","reference","attachment","mindMap"],attributes:{topicColor:{default:null,parseHTML:element=>{const value=element.getAttribute("data-topic-color");return value&&/^#[a-f0-9]{6}$/i.test(value)?value:null;},renderHTML:attrs=>attrs.topicColor?{"data-topic-color":attrs.topicColor}:{}},topicId:{default:null,parseHTML:element=>{const id=element.getAttribute("data-topic-id");return id&&/^[a-f0-9-]{36}$/i.test(id)?id:null;},renderHTML:attrs=>attrs.topicId?{"data-topic-id":attrs.topicId}:{}},topicDepth:{default:null,parseHTML:element=>{const raw=element.getAttribute("data-topic-depth");const value=Number(raw);return raw!==null&&Number.isInteger(value)&&value>=0&&value<=32?value:null;},renderHTML:attrs=>attrs.topicDepth===null?{}:{"data-topic-depth":attrs.topicDepth,style:`margin-left:${attrs.topicDepth*14}px`}}}}];},
});

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
