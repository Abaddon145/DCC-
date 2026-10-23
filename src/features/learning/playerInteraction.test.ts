import {it,expect} from "vitest";
import {classifyVideoPointer} from "./playerInteraction";

it("single click is deferred and double click never adds a pause",()=>{
  expect(classifyVideoPointer({kind:"click",inside:true},false)).toBe("deferClick");
  expect(classifyVideoPointer({kind:"click",inside:true},true)).toBe("none");
  expect(classifyVideoPointer({kind:"doubleClick",inside:true},true)).toBe("toggleFullscreen");
  expect(classifyVideoPointer({kind:"commitClick",inside:true},false)).toBe("none");
  expect(classifyVideoPointer({kind:"commitClick",inside:true},true)).toBe("togglePause");
});
it("wheel is confined to video, with input IME and overlays taking priority",()=>{
  expect(classifyVideoPointer({kind:"wheel",deltaY:-40,inside:true},false)).toBe("volumeUp");
  expect(classifyVideoPointer({kind:"wheel",deltaY:40,inside:true},false)).toBe("volumeDown");
  expect(classifyVideoPointer({kind:"wheel",deltaY:0,inside:true},false)).toBe("none");
  for(const options of [{inside:false},{inside:true,blocked:true},{inside:true,composing:true},{inside:true,editable:true}]){
    expect(classifyVideoPointer({kind:"wheel",deltaY:1,...options},false)).toBe("none");
    expect(classifyVideoPointer({kind:"click",...options},false)).toBe("none");
  }
});
