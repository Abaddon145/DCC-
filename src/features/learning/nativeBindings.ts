export interface LearningBinding {id:string;shortcut:string;scope:"learning"|"global";enabled?:boolean}
export interface NativeBinding {id:string;key:string}
export const learningBindingLabels:Record<string,string>={
  "player.toggle":"播放 / 暂停","player.seek-back":"后退 5 秒","player.seek-forward":"前进 5 秒","player.fullscreen":"切换全屏",
  "player.seek-back-arrow":"后退 5 秒","player.seek-forward-arrow":"前进 5 秒","player.seek-back-large":"后退 30 秒","player.seek-forward-large":"前进 30 秒",
  "player.mute":"静音 / 取消静音","player.fullscreen-legacy":"切换全屏","player.next":"下一课程","player.previous":"上一课程","player.exit-fullscreen":"退出全屏",
};
export const defaultLearningBindings:ReadonlyArray<LearningBinding> = [
  ["toggle","Space"],["seek-back","J"],["seek-forward","L"],["fullscreen","F11"],
  ["seek-back-arrow","ArrowLeft"],["seek-forward-arrow","ArrowRight"],
  ["seek-back-large","Shift+ArrowLeft"],["seek-forward-large","Shift+ArrowRight"],
  ["mute","M"],["fullscreen-legacy","F"],["next","N"],["previous","P"],["exit-fullscreen","Escape"],
].map(([id,shortcut])=>({id:`player.${id}`,shortcut,scope:"learning"}));

export function nativeBindings(commands:ReadonlyArray<LearningBinding>,focus:"learning"|"editor"|"dialog"):NativeBinding[] {
  if(focus!=="learning")return [];
  const seen=new Set<string>();
  const names:Record<string,string>={Space:"SPACE",ArrowLeft:"LEFT",ArrowRight:"RIGHT",ArrowUp:"UP",ArrowDown:"DOWN",Escape:"ESC",Enter:"ENTER",Tab:"TAB",Home:"HOME",End:"END",Backspace:"BS",Delete:"DEL",PageUp:"PGUP",PageDown:"PGDWN"};
  return commands.flatMap(command=>{
    if(command.scope!=="learning"||command.enabled===false)return [];
    const raw=command.shortcut.replace(/\+\+$/,"+Plus");
    const parts=raw.split("+");const key=parts.pop()||"";
    if(new Set(parts).size!==parts.length||parts.some(part=>!["Ctrl","Alt","Shift"].includes(part))||!(names[key]||/^F([1-9]|1[0-2])$/.test(key)||/^[a-zA-Z0-9]$/.test(key)||key==="Plus"))return [];
    let native=names[key]||(/^F([1-9]|1[0-2])$/.test(key)?key:"");
    if(!native){const shift=parts.includes("Shift");if(shift)parts.splice(parts.indexOf("Shift"),1);native=key==="Plus"?"+":shift&&/^\d$/.test(key)?")!@#$%^&*("[Number(key)]:shift?key:key.toLowerCase();}
    const value=[...parts,native].join("+");
    if(seen.has(value))return [];seen.add(value);return [{id:command.id,key:value}];
  });
}
