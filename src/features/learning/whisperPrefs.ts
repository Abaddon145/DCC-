export interface WhisperPrefs {
  defaultModel: "base" | "small";
  skipRepeatConsent: boolean;
}

const STORAGE_KEY = "dcc-learning-whisper-prefs";

export function loadWhisperPrefs(): WhisperPrefs {
  try {
    const raw = window.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return { defaultModel: "base", skipRepeatConsent: false };
    const parsed = JSON.parse(raw) as Partial<WhisperPrefs>;
    return {
      defaultModel: parsed.defaultModel === "small" ? "small" : "base",
      skipRepeatConsent: parsed.skipRepeatConsent === true,
    };
  } catch {
    return { defaultModel: "base", skipRepeatConsent: false };
  }
}

export function saveWhisperPrefs(next: WhisperPrefs): void {
  try {
    window.localStorage?.setItem(STORAGE_KEY, JSON.stringify(next));
    window.dispatchEvent(new CustomEvent("learning-whisper-prefs-changed", { detail: next }));
  } catch {
    // Ignore storage quota or sandbox restrictions.
  }
}
