import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { initSync, render_markdown_fast } from "../public/wasm/mdpdf_wasm.js";

const module = readFileSync(new URL("../public/wasm/mdpdf_wasm_bg.wasm", import.meta.url));
initSync({ module });

const section = `## 性能测试章节

这是用于测量 Markdown WASM 渲染吞吐量的段落，包含 **粗体**、[链接](https://example.invalid) 和 \`inline code\`。

| 列 1 | 列 2 | 列 3 | 列 4 |
|---|---|---|---|
| alpha | beta | gamma | delta |

\`\`\`rust
fn render(value: &str) -> usize { value.len() }
\`\`\`

\`\`\`mermaid
graph TD; A-->B; B-->C
\`\`\`

`;
const markdown = `# WASM Benchmark\n\n${section.repeat(800)}`;
function render(input = markdown) {
  const output = render_markdown_fast(
    input, "benchmark.md", "jp-standard", true, undefined, true, true, true, "A4",
  );
  output.take_html();
  output.free();
}

for (let index = 0; index < 3; index += 1) render();

const warmSamples = [];
for (let index = 0; index < 12; index += 1) {
  const started = performance.now();
  render();
  warmSamples.push(performance.now() - started);
}
const coldSamples = [];
for (let index = 0; index < 12; index += 1) {
  const started = performance.now();
  render(`${markdown}\nrevision-${index}`);
  coldSamples.push(performance.now() - started);
}
warmSamples.sort((left, right) => left - right);
coldSamples.sort((left, right) => left - right);
const percentile = (samples, value) => samples[
  Math.min(samples.length - 1, Math.ceil(samples.length * value) - 1)
];
const warmMedian = percentile(warmSamples, 0.5);
const coldMedian = percentile(coldSamples, 0.5);
const mib = new TextEncoder().encode(markdown).length / 1024 / 1024;

console.log(JSON.stringify({
  inputMiB: Number(mib.toFixed(3)),
  coldMedianMs: Number(coldMedian.toFixed(2)),
  coldP95Ms: Number(percentile(coldSamples, 0.95).toFixed(2)),
  coldThroughputMiBs: Number((mib / (coldMedian / 1000)).toFixed(2)),
  cachedMedianMs: Number(warmMedian.toFixed(2)),
  cachedP95Ms: Number(percentile(warmSamples, 0.95).toFixed(2)),
}));
