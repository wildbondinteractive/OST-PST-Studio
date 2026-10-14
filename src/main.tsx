import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// A right-click on an input is still useful; everywhere else the browser menu is noise
// in a desktop application and the app provides its own.
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t && (t.closest("input, textarea, [contenteditable]") || t.closest("[data-allow-context]"))) return;
  if (t && t.closest("[data-radix-context-menu-trigger], [data-context-menu]")) return;
  e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
