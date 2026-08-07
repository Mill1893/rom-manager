/**
 * The WebView entry point.
 *
 * Design tokens are written as CSS custom properties at startup rather than
 * imported as a stylesheet, so the palette has exactly one definition — the
 * TypeScript one the contrast tests check. A second copy in CSS could pass
 * those tests while shipping something else.
 */

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { toCssVariables } from "./tokens";

const style = document.createElement("style");
style.textContent = `:root {\n${toCssVariables()}\n}`;
document.head.append(style);

const container = document.querySelector("#root");
if (container === null) {
  throw new Error("the document has no #root to mount into");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
