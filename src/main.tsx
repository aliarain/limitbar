import React from "react";
import ReactDOM from "react-dom/client";
import { Widget } from "./views/Widget";
import "./styles.css";

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Widget />
  </React.StrictMode>,
);
