import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "@fontsource/space-grotesk/400.css";
import "@fontsource/space-grotesk/500.css";
import "@fontsource/space-grotesk/600.css";
import "@fontsource/space-mono/400.css";
import "@fontsource/space-mono/700.css";
import "./styles.css";

async function start() {
  // `pnpm dev` in a plain browser has no Tauri behind it: use the mock backend to design the UI.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./mock")).installMock();
  }
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
