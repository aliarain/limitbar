import React from "react";
import ReactDOM from "react-dom/client";
import { Popup } from "./views/Popup";
import "./styles.css";

// Utility window: no context menu, no text selection drag, Esc hides.
document.addEventListener("contextmenu", (e) => e.preventDefault());
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") void import("@tauri-apps/api/core").then(({ invoke }) => invoke("hide_popup"));
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Popup />
  </React.StrictMode>,
);
