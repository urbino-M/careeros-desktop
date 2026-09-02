import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { UpdateManager } from "./updates/UpdateManager";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <UpdateManager><App /></UpdateManager>
  </React.StrictMode>,
);
