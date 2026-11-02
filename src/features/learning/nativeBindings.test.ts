import {describe,it,expect} from "vitest";
import {nativeBindings,defaultLearningBindings} from "./nativeBindings";

describe("native playback keys",()=>{
  it("native_bindings_preserve_all_function_keys",()=>{
    for(let n=1;n<=12;n++)expect(nativeBindings([{id:"fullscreen",shortcut:`F${n}`,scope:"learning"}],"learning")).toEqual([{id:"fullscreen",key:`F${n}`}]);
    const values=[["Ctrl+Shift+F11","Ctrl+Shift+F11"],["Ctrl+Plus","Ctrl++"],["Ctrl++","Ctrl++"],["Ctrl+Shift+1","Ctrl+!"],["Shift+0",")"],["Shift+ArrowLeft","Shift+LEFT"],["Space","SPACE"],["M","m"]];
    for(const [shortcut,key] of values)expect(nativeBindings([{id:"fullscreen",shortcut,scope:"learning"}],"learning")).toEqual([{id:"fullscreen",key}]);
  });
  it("binds the full portable defaults, never steals editor or dialog keys",()=>{
    expect(nativeBindings(defaultLearningBindings,"learning")).toEqual([
      {id:"player.toggle",key:"SPACE"},{id:"player.seek-back",key:"j"},{id:"player.seek-forward",key:"l"},{id:"player.fullscreen",key:"F11"},
      {id:"player.seek-back-arrow",key:"LEFT"},{id:"player.seek-forward-arrow",key:"RIGHT"},{id:"player.seek-back-large",key:"Shift+LEFT"},{id:"player.seek-forward-large",key:"Shift+RIGHT"},
      {id:"player.mute",key:"m"},{id:"player.fullscreen-legacy",key:"f"},{id:"player.next",key:"n"},{id:"player.previous",key:"p"},{id:"player.exit-fullscreen",key:"ESC"},
    ]);
    expect(nativeBindings(defaultLearningBindings,"editor")).toEqual([]);
    expect(nativeBindings(defaultLearningBindings,"dialog")).toEqual([]);
    expect(nativeBindings([{id:"app.delete",scope:"global",shortcut:"Delete"},{id:"fullscreen",scope:"learning",shortcut:"F11",enabled:false}],"learning")).toEqual([]);
  });
  it("drops malformed, duplicate and injectable key specifications",()=>{
    for(const shortcut of ["F13","Ctrl+Ctrl+F11","Meta+F11","F11\nrun evil","Ctrl+; run",""])
      expect(nativeBindings([{id:"fullscreen",scope:"learning",shortcut}],"learning")).toEqual([]);
    expect(nativeBindings([{id:"fullscreen",scope:"learning",shortcut:"F11"},{id:"other",scope:"learning",shortcut:"F11"}],"learning")).toHaveLength(1);
  });
});
