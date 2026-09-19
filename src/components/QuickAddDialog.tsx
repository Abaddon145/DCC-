import { useState } from "react";
import { ClipboardPaste, Link2, X } from "lucide-react";
import { api } from "../lib/api";
import type { ParsedShareText } from "../types";

interface Props { onClose: () => void; onParsed: (value: ParsedShareText) => void; onOpenExisting: (id: string) => void; notify: (message: string, error?: boolean) => void }

export function QuickAddDialog({ onClose, onParsed, onOpenExisting, notify }: Props) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const parse = async () => {
    if (!text.trim()) return;
    setBusy(true);
    try {
      const parsed = await api.parseShareText(text);
      const duplicate = await api.checkShareUrl(parsed.shareUrl);
      if (duplicate) {
        if (window.confirm(`这个链接已属于“${duplicate.name}”。是否打开现有素材？`)) onOpenExisting(duplicate.id);
        return;
      }
      onParsed(parsed);
    } catch (error) { notify(String(error), true); }
    finally { setBusy(false); }
  };
  const paste = async () => {
    try { setText(await api.readClipboard()); } catch (error) { notify(String(error), true); }
  };
  return <div className="modal-backdrop"><section className="quick-modal" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">QUICK CAPTURE</span><h2>快速录入百度网盘素材</h2></div><button className="icon-button" onClick={onClose}><X size={19} /></button></header>
    <div className="quick-body"><div className="quick-icon"><Link2 size={24} /></div><p>粘贴包含分享链接和提取码的整段文字，识别后会进入完整素材表单。</p><textarea autoFocus rows={7} value={text} onChange={event => setText(event.target.value)} placeholder={'链接：https://pan.baidu.com/s/...\n提取码：a1b2'} /><button className="secondary-button paste-button" onClick={paste}><ClipboardPaste size={16} />从剪贴板粘贴</button></div>
    <footer className="modal-footer"><button className="secondary-button" onClick={onClose}>取消</button><button className="primary-button" disabled={!text.trim() || busy} onClick={parse}>{busy ? "正在识别…" : "识别并继续"}</button></footer>
  </section></div>;
}
