export type PlaybackIntent="none"|"deferClick"|"togglePause"|"toggleFullscreen"|"volumeUp"|"volumeDown"|"contextMenu";
export interface VideoPointer {kind:"click"|"doubleClick"|"commitClick"|"wheel"|"context";inside:boolean;deltaY?:number;doubleClickMs?:number;blocked?:boolean;composing?:boolean;editable?:boolean}
export function classifyVideoPointer(event:VideoPointer,pendingClick:boolean):PlaybackIntent {
  if(!event.inside||event.blocked||event.composing||event.editable)return "none";
  switch(event.kind){
    case "click":return pendingClick?"none":"deferClick";
    case "commitClick":return pendingClick?"togglePause":"none";
    case "doubleClick":return "toggleFullscreen";
    case "context":return "contextMenu";
    case "wheel":return (event.deltaY||0)<0?"volumeUp":(event.deltaY||0)>0?"volumeDown":"none";
  }
}
