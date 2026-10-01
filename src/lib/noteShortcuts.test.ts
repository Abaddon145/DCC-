import {describe,it,expect} from "vitest";
import {noteShortcutAction} from "./noteShortcuts";
describe("contextual note shortcuts",()=>{
  it("handles shifted digits and plus on Windows keyboards",()=>{
    expect(noteShortcutAction(new KeyboardEvent("keydown",{key:"!",code:"Digit1",altKey:true,shiftKey:true}))).toBe("noteSiblingsHeading1");
    expect(noteShortcutAction(new KeyboardEvent("keydown",{key:"+",code:"Equal",ctrlKey:true,shiftKey:true}))).toBe("noteZoomIn");
    expect(noteShortcutAction(new KeyboardEvent("keydown",{key:"Enter",ctrlKey:true}))).toBe("noteStrike");
  });
  it("supports replacing default shortcuts without enabling the old key",()=>{
    expect(noteShortcutAction(new KeyboardEvent("keydown",{key:"k",ctrlKey:true}),{noteLink:"Alt+K"})).toBeUndefined();
    expect(noteShortcutAction(new KeyboardEvent("keydown",{key:"k",altKey:true}),{noteLink:"Alt+K"})).toBe("noteLink");
  });
});
