import { readFileSync } from "node:fs";
import { initSync, render_markdown_fast } from "../public/wasm/mdpdf_wasm.js";

const module = readFileSync(new URL("../public/wasm/mdpdf_wasm_bg.wasm", import.meta.url));
initSync({ module });

const output = render_markdown_fast(
  "# WASM smoke\n\n## Section\n\n| A | B |\n|---|---|\n| 1 | 2 |",
  "smoke.md", "jp-standard", false, undefined, false, true, false, "A4", "",
);
const result = {
  html: output.take_html(),
  warnings: JSON.parse(output.take_warnings_json()),
  logs: JSON.parse(output.take_logs_json()),
};
output.free();

if (!result.html.includes("WASM smoke") || !result.html.includes("class=\"doc-toc\"")) {
  throw new Error("WASM renderer smoke test failed");
}

console.log("WASM renderer smoke test passed");
