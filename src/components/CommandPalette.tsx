import { useEffect, useMemo, useRef, useState } from "react";
import { Command, Search } from "lucide-react";

export interface PaletteCommand { id: string; label: string; detail?: string; shortcut?: string; run: () => void }
export function CommandPalette({ commands, onClose }: { commands: PaletteCommand[]; onClose: () => void }) {
  const [query, setQuery] = useState(""); const [active, setActive] = useState(0); const input = useRef<HTMLInputElement>(null);
  const filtered = useMemo(() => { const words = query.toLowerCase().split(/\s+/).filter(Boolean); return commands.filter(command => words.every(word => `${command.label} ${command.detail || ""}`.toLowerCase().includes(word))); }, [commands, query]);
  useEffect(() => { setActive(0); }, [query]);
  useEffect(() => { input.current?.focus(); const handler = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); else if (event.key === "ArrowDown") { event.preventDefault(); setActive(value => Math.min(filtered.length - 1, value + 1)); } else if (event.key === "ArrowUp") { event.preventDefault(); setActive(value => Math.max(0, value - 1)); } else if (event.key === "Enter" && filtered[active]) { event.preventDefault(); filtered[active].run(); onClose(); } }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, [onClose, filtered, active]);
  return <div className="command-backdrop" onMouseDown={event => event.target === event.currentTarget && onClose()}><section className="command-palette"><header><Command size={18} /><input ref={input} value={query} onChange={event => setQuery(event.target.value)} placeholder="输入命令或功能名称…" /></header><div>{filtered.map((command, index) => <button key={command.id} className={index === active ? "active" : ""} onMouseEnter={() => setActive(index)} onClick={() => { command.run(); onClose(); }}><Search size={14} /><span><strong>{command.label}</strong>{command.detail && <small>{command.detail}</small>}</span>{command.shortcut && <kbd>{command.shortcut}</kbd>}</button>)}{!filtered.length && <p>没有匹配的命令</p>}</div></section></div>;
}
