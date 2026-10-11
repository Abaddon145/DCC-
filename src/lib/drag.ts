import type { LibraryDragPayload } from "../types";

export interface LibraryDragPoint { x: number; y: number }

export type BeginLibraryPointerDrag = (
  payload: LibraryDragPayload,
  event: React.PointerEvent<HTMLElement>
) => void;

export function dragPreviewText(payload: LibraryDragPayload) {
  return payload.kind === "assets" && payload.ids.length > 1
    ? `${payload.ids.length} 项素材`
    : payload.label;
}

/** Pointer position stays authoritative even while the mouse is stationary. */
export function beginEdgeScroll(onScroll: () => void = () => {}) {
  let point: LibraryDragPoint | null = null;
  let frame = 0;
  let previous = 0;
  let stopped = false;
  const tick = (time: number) => {
    if (stopped) return;
    const elapsed = previous ? Math.min(32, time - previous) : 16;
    previous = time;
    if (point) {
      let node = document.elementFromPoint(point.x, point.y) as HTMLElement | null;
      while (node) {
        const style = getComputedStyle(node);
        const rect = node.getBoundingClientRect();
        const vertical = node.scrollHeight > node.clientHeight && /auto|scroll/.test(style.overflowY);
        const horizontal = node.scrollWidth > node.clientWidth && /auto|scroll/.test(style.overflowX);
        const dy = vertical ? edgeScrollSpeed(point.y, rect.top, rect.bottom) : 0;
        const dx = horizontal ? edgeScrollSpeed(point.x, rect.left, rect.right) : 0;
        if (dy || dx) {
          const before = [node.scrollTop, node.scrollLeft];
          node.scrollTop += dy * elapsed / 16;
          node.scrollLeft += dx * elapsed / 16;
          if (node.scrollTop !== before[0] || node.scrollLeft !== before[1]) { onScroll(); break; }
        }
        node = node.parentElement;
      }
    }
    frame = requestAnimationFrame(tick);
  };
  return {
    update(next: LibraryDragPoint) { point = next; if (!frame && !stopped) frame = requestAnimationFrame(tick); },
    stop() { stopped = true; cancelAnimationFrame(frame); point = null; },
  };
}

export function edgeScrollSpeed(position: number, start: number, end: number) {
  if (position < start || position > end) return 0;
  const edge = Math.min(40, (end - start) / 3);
  if (edge <= 0) return 0;
  if (position < start + edge) return -14 * (1 - (position - start) / edge);
  if (position > end - edge) return 14 * (1 - (end - position) / edge);
  return 0;
}

/** Keep source geometry intact; a rejected move never alters the list. */
export function beginNavigationFeedback(source: HTMLElement) {
  source.classList.add("navigation-drag-source");
  return (accepted: boolean) => {
    source.classList.remove("navigation-drag-source");
    if (!accepted) {
      source.classList.add("navigation-snap-back");
      window.setTimeout(() => source.classList.remove("navigation-snap-back"), 220);
    }
  };
}

/** Navigation lists share pointer feedback; callers own validation and persistence. */
export function beginPointerReorder(event: Pick<React.PointerEvent<HTMLElement>,"button"|"clientX"|"clientY"|"pointerId"|"currentTarget">, options: {
  selector:string;
  source?:HTMLElement;
  placement?:(target:HTMLElement,point:LibraryDragPoint)=>"before"|"inside"|"after";
  canDrop?:(target:HTMLElement,zone:"before"|"inside"|"after")=>boolean;
  onDrop:(target:HTMLElement,after:boolean,zone:"before"|"inside"|"after")=>void;
  onInvalid?:()=>void;
  onEdge?:(point:LibraryDragPoint)=>void;
  onFinish?:()=>void;
  onStart?:()=>void;
  onHover?:(target:HTMLElement|null,zone:"before"|"inside"|"after",valid:boolean)=>void;
}) {
  if(event.button!==0)return()=>{};
  const source=options.source||event.currentTarget;const start={x:event.clientX,y:event.clientY};const pointerId=event.pointerId;
  let active=false;let last=start;let target:HTMLElement|null=null;let after=false;
  let finished=false;
  let preview:HTMLElement|null=null;let placeholder:HTMLElement|null=null;
  let zone:"before"|"inside"|"after"="before";let valid=false;
  let highlighted:HTMLElement|null=null;
  const display=source.style.display;
  const locate=()=>{
    const hit=document.elementFromPoint(last.x,last.y);
    if(hit?.closest(".navigation-placeholder"))return;
    const row=hit?.closest<HTMLElement>(options.selector);
    target=row&&row!==source?row:null;
    highlighted?.classList.remove("navigation-drop-inside","navigation-drop-invalid");highlighted=null;
    if(target&&placeholder){
      const rect=target.getBoundingClientRect();
      const style=getComputedStyle(target.parentElement!);
      const horizontal=style.display==="flex"&&style.flexDirection==="row";
      after=horizontal?last.x>rect.left+rect.width/2:last.y>rect.top+rect.height/2;
      zone=options.placement?.(target,last)||(after?"after":"before");
      after=zone==="after";
      valid=options.canDrop?.(target,zone)??true;
      options.onHover?.(target,zone,valid);
      if(!valid){highlighted=target;target.classList.add("navigation-drop-invalid");return;}
      if(zone==="inside"){highlighted=target;target.classList.add("navigation-drop-inside");return;}
      if(target.hasAttribute("data-task-column")){target.append(placeholder);after=true;return;}
      target.parentElement?.insertBefore(placeholder,after?target.nextSibling:target);
    }else {valid=false;options.onHover?.(null,zone,false);}
  };
  const edge=beginEdgeScroll(()=>locate());
  const cleanup=()=>{edge.stop();options.onFinish?.();highlighted?.classList.remove("navigation-drop-inside","navigation-drop-invalid");window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",up);window.removeEventListener("pointercancel",cancel);window.removeEventListener("keydown",key);preview?.remove();placeholder?.remove();source.style.display=display;document.body.classList.remove("library-pointer-dragging");};
  const cancel=()=>{if(finished)return;finished=true;cleanup();if(active)beginNavigationFeedback(source)(false);};
  const move=(e:PointerEvent)=>{
    if(e.pointerId!==pointerId)return;last={x:e.clientX,y:e.clientY};
    if(!active&&Math.hypot(last.x-start.x,last.y-start.y)<6)return;
    if(!active){
      active=true;options.onStart?.();const rect=source.getBoundingClientRect();
      preview=source.cloneNode(true) as HTMLElement;preview.classList.add("navigation-floating-preview");preview.removeAttribute("id");preview.style.width=`${rect.width}px`;preview.style.height=`${rect.height}px`;preview.style.maxHeight="180px";preview.setAttribute("aria-hidden","true");document.body.append(preview);
      placeholder=document.createElement("div");placeholder.className="navigation-placeholder";placeholder.style.height=`${rect.height}px`;placeholder.style.width=`${rect.width}px`;placeholder.setAttribute("aria-hidden","true");source.after(placeholder);source.style.display="none";document.body.classList.add("library-pointer-dragging");
    }
    e.preventDefault();if(preview){preview.style.left=`${last.x+12}px`;preview.style.top=`${last.y-8}px`;}
    locate();edge.update(last);options.onEdge?.(last);
  };
  const up=(e:PointerEvent)=>{
    if(finished||e.pointerId!==pointerId)return;finished=true;last={x:e.clientX,y:e.clientY};if(active)locate();const destination=target;cleanup();
    if(!active)return;
    const block=(e:MouseEvent)=>{e.preventDefault();e.stopImmediatePropagation();};window.addEventListener("click",block,{capture:true,once:true});window.setTimeout(()=>window.removeEventListener("click",block,true),100);
    if(destination&&valid)options.onDrop(destination,after,zone);else{beginNavigationFeedback(source)(false);options.onInvalid?.();}
  };
  const key=(e:KeyboardEvent)=>{if(e.key==="Escape")cancel();};
  window.addEventListener("pointermove",move,{passive:false});window.addEventListener("pointerup",up);window.addEventListener("pointercancel",cancel);window.addEventListener("keydown",key);
  return cancel;
}
