import type { Editor } from "@tiptap/react";
import { Slice } from "@tiptap/pm/model";
import { knowledgeApi } from "./knowledge";

/** Both context menus and keyboard events use the exact same ProseMirror slice. */
export async function copyNoteSelection(editor: Editor, libraryId: string, noteId: string, cut: boolean) {
  const selection = editor.state.selection;
  if (selection.empty) return;
  const fragment = selection.content();
  const before = JSON.stringify(fragment.toJSON());
  await knowledgeApi(libraryId).writeFragment(noteId, fragment.toJSON());
  if (cut && !editor.isDestroyed && editor.isEditable && editor.state.selection.from === selection.from && editor.state.selection.to === selection.to && JSON.stringify(editor.state.selection.content().toJSON()) === before) {
    editor.chain().focus().deleteSelection().run();
  }
}

export async function pasteNoteSelection(editor: Editor, libraryId: string, noteId: string): Promise<boolean> {
  const bookmark = editor.state.selection.getBookmark();
  const doc = editor.state.doc;
  const fragment = await knowledgeApi(libraryId).readFragment(noteId);
  if (!fragment) return false;
  if (editor.isDestroyed || !editor.isEditable || !editor.state.doc.eq(doc)) throw new Error("正文已变化，请重新粘贴");
  const transaction = editor.state.tr.setSelection(bookmark.resolve(editor.state.doc)).replaceSelection(Slice.fromJSON(editor.schema, fragment));
  editor.view.dispatch(transaction.scrollIntoView());
  editor.commands.focus();
  return true;
}
