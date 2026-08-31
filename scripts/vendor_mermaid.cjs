const { copyFileSync, mkdirSync } = require("node:fs");
const { dirname, join } = require("node:path");

const root = join(__dirname, "..");
const files = [
  ["node_modules/mermaid/dist/mermaid.min.js", "public/vendor/mermaid.min.js"],
  ["node_modules/mermaid/LICENSE", "public/vendor/MERMAID-LICENSE"],
  ["node_modules/mermaid/dist/mermaid.min.js", "themes/common/mermaid.min.js"],
  ["node_modules/mermaid/LICENSE", "themes/common/MERMAID-LICENSE"],
];

for (const [source, target] of files) {
  const output = join(root, target);
  mkdirSync(dirname(output), { recursive: true });
  copyFileSync(join(root, source), output);
  process.stdout.write(`vendored ${target}\n`);
}
