import {LoaderCircle,Search} from "lucide-react";
export function UrlInput({url,busy,onChange,onParse}:{url:string;busy:boolean;onChange:(value:string)=>void;onParse:()=>void}) {
 return <form className="download-url" onSubmit={event=>{event.preventDefault();if(!busy&&url.trim())onParse();}}><label htmlFor="download-url">视频链接</label><div><input id="download-url" type="url" value={url} disabled={busy} onChange={event=>onChange(event.target.value)} placeholder="粘贴 YouTube、哔哩哔哩或其他视频链接"/><button className="primary-button" disabled={busy||!url.trim()} type="submit">{busy?<LoaderCircle size={16} className="download-spinner" aria-hidden="true"/>:<Search size={16} aria-hidden="true"/>}{busy?"正在解析…":"解析"}</button></div><small>仅下载您有权保存的内容。不绕过 DRM、付费或访问限制。</small></form>;
}
