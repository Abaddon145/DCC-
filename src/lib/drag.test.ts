import {describe, expect, it,vi} from "vitest";
import {edgeScrollSpeed,beginPointerReorder} from "./drag";

describe("navigation edge scrolling", () => {
  it("scrolls continuously only inside the edge, with bounded speed", () => {
    expect(edgeScrollSpeed(0, 0, 200)).toBe(-14);
    expect(edgeScrollSpeed(200, 0, 200)).toBe(14);
    expect(edgeScrollSpeed(100, 0, 200)).toBe(0);
    expect(edgeScrollSpeed(-1, 0, 200)).toBe(0);
    expect(edgeScrollSpeed(201, 0, 200)).toBe(0);
    expect(edgeScrollSpeed(20, 0, 200)).toBe(-7);
  });
  it("keeps an equal placeholder, commits once and cleans up cancelled drops",()=>{
    const source=document.createElement("div"),target=document.createElement("div"),list=document.createElement("div");
    source.dataset.row="source";target.dataset.row="target";list.append(source,target);document.body.append(list);
    Object.defineProperty(document,"elementFromPoint",{configurable:true,value:()=>target});
    const rect={x:0,y:0,top:0,left:0,right:100,bottom:40,width:100,height:40,toJSON:()=>({})};
    source.getBoundingClientRect=()=>rect;target.getBoundingClientRect=()=>rect;
    const send=(name:string)=>{const event=new MouseEvent(name,{clientX:20,clientY:30,bubbles:true});Object.defineProperty(event,"pointerId",{value:1});window.dispatchEvent(event);};
    const event={button:0,clientX:0,clientY:0,pointerId:1,currentTarget:source};
    const commit=vi.fn(),finish=vi.fn();
    const cancel=beginPointerReorder(event,{selector:"[data-row]",onDrop:commit,onFinish:finish});
    send("pointermove");expect(source.style.display).toBe("none");expect(document.querySelector<HTMLElement>(".navigation-placeholder")?.style.height).toBe("40px");
    send("pointerup");cancel();expect(commit).toHaveBeenCalledTimes(1);expect(finish).toHaveBeenCalledTimes(1);expect(source.style.display).toBe("");expect(document.querySelector(".navigation-placeholder")).toBeNull();
    const rejected=vi.fn();const stop=beginPointerReorder(event,{selector:"[data-row]",canDrop:()=>false,onDrop:rejected});send("pointermove");expect(target.classList.contains("navigation-drop-invalid")).toBe(true);stop();expect(rejected).not.toHaveBeenCalled();expect(source.style.display).toBe("");expect(document.querySelector(".navigation-floating-preview")).toBeNull();
    list.remove();
  });
});
