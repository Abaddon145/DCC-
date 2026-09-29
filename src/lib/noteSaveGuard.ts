const guards = new Set<() => Promise<void>>();
const listeners = new Set<() => void>();
let barriers = 0;
export function registerNoteSaveGuard(guard: () => Promise<void>) { guards.add(guard); return () => { guards.delete(guard); }; }
export async function flushKnowledgeEdits() { for (const guard of guards) await guard(); }
export const knowledgeChangesPaused = () => barriers > 0;
export function subscribeKnowledgeBarrier(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }
export async function withKnowledgeBarrier<T>(action: () => Promise<T>): Promise<T> {
  barriers++; listeners.forEach(listener => listener());
  try { await flushKnowledgeEdits(); return await action(); }
  finally { barriers--; listeners.forEach(listener => listener()); }
}
