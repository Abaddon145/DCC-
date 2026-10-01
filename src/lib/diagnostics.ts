export type DiagnosticCode = "render-failure" | "read-failure" | "save-failure" | "unhandled-error";
const key = "dcc-diagnostics-v1";
export function readDiagnostics(): Array<{time:string;code:DiagnosticCode}> {
  try { const value:unknown=JSON.parse(localStorage.getItem(key)||"[]"); return Array.isArray(value)?value.filter(v=>v&&typeof v.time==="string"&&["render-failure","read-failure","save-failure","unhandled-error"].includes(v.code)).map(v=>({time:v.time,code:v.code})).slice(-100):[]; } catch { return []; }
}
// Only fixed event codes are retained. Never store raw exceptions, paths, content or credentials.
export function recordDiagnostic(code:DiagnosticCode) {
  try { localStorage.setItem(key,JSON.stringify([...readDiagnostics(),{time:new Date().toISOString(),code}].slice(-100))); } catch { /* A full disk must not prevent the original operation's error display. */ }
}
