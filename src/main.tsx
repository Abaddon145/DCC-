import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App";
import { ReferenceWindowApp } from "./components/ReferenceWindowApp";
import "./styles.css";
import { api } from "./lib/api";
import { applyGlobalPreferences, defaultGlobalPreferences } from "./lib/personalization";
import { listen } from "@tauri-apps/api/event";
import type { GlobalPreferences } from "./types";

applyGlobalPreferences(defaultGlobalPreferences);
if ("__TAURI_INTERNALS__" in window) {
  void api.getPersonalization().then(value => applyGlobalPreferences(value.global)).catch(() => undefined);
  void listen<GlobalPreferences>("personalization-changed", event => applyGlobalPreferences(event.payload));
}

const referenceWindow = "__TAURI_INTERNALS__" in window && getCurrentWebviewWindow().label === "reference-board";
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {referenceWindow ? <ReferenceWindowApp /> : <App />}
  </StrictMode>
);
