import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../shared/styles/tokens.css";
import "../shared/styles/reset.css";
import "../shared/ui/ui.css";
import { App } from "./App";

async function main() {
  // Outside Tauri there is no backend, so the browser runs against the mock.
  if (!("__TAURI_INTERNALS__" in window)) {
    const { installMock } = await import("../shared/ipc/mock");
    installMock();
  }

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void main();
