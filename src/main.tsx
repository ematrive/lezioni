import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "./lib/theme";
import { useUiLang } from "./i18n";

/** A change of language mounts the app again: every label is read once more. */
function Root() {
  const lang = useUiLang();
  return <App key={lang} />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
