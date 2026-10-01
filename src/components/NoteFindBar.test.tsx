import {Editor} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import {describe,it,expect} from "vitest";
import {noteTextMatches,NoteSearchHighlights} from "./NoteFindBar";

describe("note text search",()=>{
  it("finds text across marks without changing content or history",()=>{
    const editor=new Editor({extensions:[StarterKit,NoteSearchHighlights],content:{type:"doc",content:[{type:"paragraph",content:[{type:"text",text:"Skeletal "},{type:"text",text:"Mesh",marks:[{type:"bold"}]}]},{type:"heading",attrs:{level:2},content:[{type:"text",text:"Mesh"}]}]}});
    try{
      const before=editor.getJSON();
      expect(noteTextMatches(editor,"skeletal mesh")).toEqual([{from:1,to:14,block:0}]);
      expect(noteTextMatches(editor,"mesh").map(v=>v.block)).toEqual([0,1]);
      expect(noteTextMatches(editor,"")).toEqual([]);
      expect(editor.getJSON()).toEqual(before);
    }finally{editor.destroy();}
  });
});
