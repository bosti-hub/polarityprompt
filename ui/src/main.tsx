import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { isTauri } from "./ipc";
import "./styles.css";

// In the installed app the webview should not behave like a browser: no Back/Reload/
// Print context menu, and no reload or print keys (a reload mid-take loses the
// talent window's motion). Text fields keep their own menu for copy/paste.
// Dev builds keep everything, including the inspector.
if (isTauri() && !import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => {
    const t = e.target as HTMLElement | null;
    if (!t?.closest("input, textarea")) e.preventDefault();
  });
  window.addEventListener("keydown", (e) => {
    const k = e.key.toLowerCase();
    const ctrl = e.ctrlKey || e.metaKey;
    if (k === "f5" || (ctrl && (k === "r" || k === "p" || k === "f" || k === "g" || k === "u"))) e.preventDefault();
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
