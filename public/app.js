import initWasm, { render_markdown_fast as renderMarkdownWasm } from "./wasm/mdpdf_wasm.js";

const invoke = window.__TAURI__?.core?.invoke;
const wasmReady = initWasm();

const elements = Object.fromEntries([
  "openFolderBtn", "refreshBtn", "workspacePath", "fileFilter", "selectAllFiles",
  "selectionCount", "fileList", "activeFilename", "activeRelativePath", "dirtyState",
  "reloadBtn", "saveBtn", "markdownEditor", "autoSaveToggle", "editorStats",
  "settingsToggle", "settingsPanel", "themeSelect", "pageSizeSelect",
  "fontFamilySelect", "fontSizeInput", "lineHeightInput", "textColorInput", "accentColorInput",
  "marginTopInput", "marginRightInput", "marginBottomInput", "marginLeftInput",
  "headerEnabledToggle", "headerTextInput", "headerAlignSelect", "footerEnabledToggle",
  "footerTextInput", "footerAlignSelect", "customCssInput", "resetStyleBtn", "formatToolbar",
  "mermaidToggle", "coverToggle", "tocToggle", "chapterBreakToggle", "previewFrame",
  "operationState", "batchFind", "batchReplace", "caseSensitive",
  "previewReplaceBtn", "applyReplaceBtn", "exportSelectedBtn", "batchResultDialog",
  "batchResultTitle", "batchResultBody",
].map((id) => [id, document.getElementById(id)]));

const state = {
  root: "",
  documents: [],
  selected: new Set(),
  active: null,
  filter: "",
  lastBatchPreviewKey: "",
  busy: false,
};

let previewTimer = 0;
let saveTimer = 0;
let previewSequence = 0;
let visualSyncTimer = 0;
let mermaidEditSequence = 0;

const STYLE_DEFAULTS = Object.freeze({
  theme: "custom",
  fontFamily: "sans",
  fontSize: 10.5,
  lineHeight: 1.7,
  textColor: "#242424",
  accentColor: "#2f6f73",
  marginTop: 20,
  marginRight: 18,
  marginBottom: 18,
  marginLeft: 18,
  headerEnabled: false,
  headerText: "",
  headerAlign: "left",
  footerEnabled: true,
  footerText: "{page} / {total}",
  footerAlign: "right",
  customCss: "",
});

const FONT_STACKS = Object.freeze({
  sans: '"Inter", "Segoe UI", "Microsoft YaHei", system-ui, sans-serif',
  serif: '"Noto Serif CJK SC", "Source Han Serif SC", "SimSun", serif',
  jp: '"Noto Sans JP", "Yu Gothic", "Meiryo", sans-serif',
  mono: '"JetBrains Mono", "Cascadia Code", "Consolas", monospace',
});

const PAGE_DIMENSIONS = Object.freeze({
  A3: [297, 420],
  A4: [210, 297],
  Letter: [215.9, 279.4],
});

const mermaid = window.mermaid;
if (mermaid) {
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    suppressErrorRendering: true,
  });
}

function requireDesktop() {
  if (typeof invoke !== "function") {
    throw new Error("この画面は Tauri デスクトップアプリ内で実行してください");
  }
}

async function call(command, args = {}) {
  requireDesktop();
  try {
    return await invoke(command, args);
  } catch (error) {
    const message = typeof error === "string" ? error : (error?.message || String(error));
    setStatus(message, true);
    throw new Error(message);
  }
}

async function chooseWorkspace() {
  if (!(await flushDirty())) return;
  setStatus("フォルダを読み込み中…");
  const snapshot = await call("choose_workspace");
  if (snapshot) applySnapshot(snapshot);
  setStatus("準備完了");
}

async function refreshWorkspace({ quiet = false } = {}) {
  if (!state.root || state.busy) return;
  if (!quiet) setStatus("一覧を更新中…");
  const snapshot = await call("scan_workspace");
  if (snapshot) {
    const activeEntry = state.active && snapshot.documents.find((doc) => doc.path === state.active.path);
    applySnapshot(snapshot);
    if (activeEntry && !state.active.dirty && activeEntry.modifiedMs !== state.active.modifiedMs) {
      await openDocument(activeEntry.path, { force: true });
    }
  }
  if (!quiet) setStatus("準備完了");
}

function applySnapshot(snapshot) {
  state.root = snapshot.root;
  state.documents = snapshot.documents;
  const validPaths = new Set(snapshot.documents.map((doc) => doc.path));
  state.selected = new Set([...state.selected].filter((path) => validPaths.has(path)));
  elements.workspacePath.textContent = snapshot.root;
  elements.workspacePath.title = snapshot.root;
  renderFileList();
}

function visibleDocuments() {
  const needle = state.filter.trim().toLocaleLowerCase();
  return needle
    ? state.documents.filter((doc) => doc.relativePath.toLocaleLowerCase().includes(needle))
    : state.documents;
}

function renderFileList() {
  const visible = visibleDocuments();
  elements.fileList.replaceChildren();
  if (!visible.length) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = state.root ? "Markdown ファイルがありません。" : "";
    elements.fileList.append(empty);
  }
  const fragment = document.createDocumentFragment();
  for (const documentEntry of visible) {
    const row = document.createElement("div");
    row.className = "file-row";
    row.classList.toggle("active", state.active?.path === documentEntry.path);
    row.setAttribute("role", "option");
    row.title = documentEntry.relativePath;

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = state.selected.has(documentEntry.path);
    checkbox.addEventListener("click", (event) => event.stopPropagation());
    checkbox.addEventListener("change", () => toggleSelection(documentEntry.path, checkbox.checked));

    const text = document.createElement("span");
    const name = document.createElement("strong");
    const relative = document.createElement("small");
    name.textContent = documentEntry.filename;
    relative.textContent = `${documentEntry.relativePath} · ${formatBytes(documentEntry.size)}`;
    text.append(name, relative);
    row.append(checkbox, text);
    row.addEventListener("click", () => openDocument(documentEntry.path));
    fragment.append(row);
  }
  elements.fileList.append(fragment);
  updateSelectionState();
}

function toggleSelection(path, selected) {
  if (selected) state.selected.add(path);
  else state.selected.delete(path);
  state.lastBatchPreviewKey = "";
  elements.applyReplaceBtn.disabled = true;
  updateSelectionState();
}

function updateSelectionState() {
  const visible = visibleDocuments();
  const selectedVisible = visible.filter((doc) => state.selected.has(doc.path)).length;
  elements.selectionCount.textContent = `${state.selected.size} / ${state.documents.length}`;
  elements.selectAllFiles.checked = visible.length > 0 && selectedVisible === visible.length;
  elements.selectAllFiles.indeterminate = selectedVisible > 0 && selectedVisible < visible.length;
}

async function openDocument(path, { force = false } = {}) {
  if (!force && state.active?.path === path) return;
  if (!force && !(await flushDirty())) return;
  setStatus("文書を読み込み中…");
  const result = await call("read_document", { request: { path } });
  const entry = state.documents.find((doc) => doc.path === path);
  state.active = {
    path: result.path,
    relativePath: entry?.relativePath || result.path,
    filename: entry?.filename || result.path.split(/[\\/]/).pop(),
    content: result.content,
    modifiedMs: result.modifiedMs,
    dirty: false,
  };
  elements.markdownEditor.value = result.content;
  elements.markdownEditor.disabled = false;
  elements.saveBtn.disabled = false;
  elements.reloadBtn.disabled = false;
  elements.activeFilename.textContent = state.active.filename;
  elements.activeRelativePath.textContent = state.active.relativePath;
  renderFileList();
  updateEditorStats();
  setDirtyState("保存済み", false);
  await updatePreview();
  setStatus("準備完了");
}

async function saveActive() {
  syncVisualEditor();
  if (!state.active?.dirty) return true;
  clearTimeout(saveTimer);
  try {
    setDirtyState("保存中…", false);
    const result = await call("save_document", {
      request: {
        path: state.active.path,
        content: elements.markdownEditor.value,
        expectedModifiedMs: state.active.modifiedMs,
      },
    });
    state.active.content = elements.markdownEditor.value;
    state.active.modifiedMs = result.modifiedMs;
    state.active.dirty = false;
    setDirtyState("保存済み", false);
    return true;
  } catch (error) {
    setDirtyState("保存失敗", true);
    return false;
  }
}

async function flushDirty() {
  if (!state.active?.dirty) return true;
  if (elements.autoSaveToggle.checked) return saveActive();
  if (!window.confirm("現在の変更を保存してから移動しますか？")) return false;
  return saveActive();
}

function markDirty(refreshPreview = true) {
  if (!state.active) return;
  state.active.dirty = true;
  setDirtyState("未保存", true);
  updateEditorStats();
  if (refreshPreview) {
    clearTimeout(previewTimer);
    previewTimer = window.setTimeout(updatePreview, 120);
  }
  if (elements.autoSaveToggle.checked) {
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(saveActive, 850);
  }
}

function effectiveTheme() {
  return elements.themeSelect.value === "custom" ? "jp-standard" : elements.themeSelect.value;
}

function boundedNumber(element, fallback, min, max) {
  const value = Number.parseFloat(element.value);
  return Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

function cssString(value) {
  return `"${String(value)
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"')
    .replaceAll("\r", "")
    .replaceAll("\n", "\\A ")}"`;
}

function pageCounterContent(value) {
  const parts = [];
  const pattern = /\{page\}|\{total\}/g;
  let cursor = 0;
  for (const match of value.matchAll(pattern)) {
    if (match.index > cursor) parts.push(cssString(value.slice(cursor, match.index)));
    parts.push(match[0] === "{page}" ? "counter(page)" : "counter(pages)");
    cursor = match.index + match[0].length;
  }
  if (cursor < value.length) parts.push(cssString(value.slice(cursor)));
  return parts.length ? parts.join(" ") : '""';
}

function previewMarginBox(edge, enabled, value, align) {
  if (!enabled || !value.trim()) return "";
  return `  @${edge}-${align} {\n    content: ${pageCounterContent(value)};\n    color: #606a73;\n    font-size: 8pt;\n  }`;
}

function composeCustomCss() {
  const fontFamily = FONT_STACKS[elements.fontFamilySelect.value] || FONT_STACKS.sans;
  const fontSize = boundedNumber(elements.fontSizeInput, STYLE_DEFAULTS.fontSize, 8, 24);
  const lineHeight = boundedNumber(elements.lineHeightInput, STYLE_DEFAULTS.lineHeight, 1, 2.5);
  const margins = {
    top: boundedNumber(elements.marginTopInput, STYLE_DEFAULTS.marginTop, 0, 60),
    right: boundedNumber(elements.marginRightInput, STYLE_DEFAULTS.marginRight, 0, 60),
    bottom: boundedNumber(elements.marginBottomInput, STYLE_DEFAULTS.marginBottom, 0, 60),
    left: boundedNumber(elements.marginLeftInput, STYLE_DEFAULTS.marginLeft, 0, 60),
  };
  const headerText = elements.headerTextInput.value.trim();
  const footerText = elements.footerTextInput.value.trim();
  const screenHeader = headerText.replaceAll("{page}", "1").replaceAll("{total}", "…");
  const screenFooter = footerText.replaceAll("{page}", "1").replaceAll("{total}", "…");
  const marginBoxes = [
    previewMarginBox("top", elements.headerEnabledToggle.checked, headerText, elements.headerAlignSelect.value),
    previewMarginBox("bottom", elements.footerEnabledToggle.checked, footerText, elements.footerAlignSelect.value),
  ].filter(Boolean).join("\n");
  const custom = elements.customCssInput.value.trim();
  const pageDimensions = PAGE_DIMENSIONS[elements.pageSizeSelect.value] || PAGE_DIMENSIONS.A4;

  return `
:root {
  --ink: ${elements.textColorInput.value};
  --accent: ${elements.accentColorInput.value};
}
body {
  color: ${elements.textColorInput.value};
  font-family: ${fontFamily};
  font-size: ${fontSize}pt;
  line-height: ${lineHeight};
}
@page {
  size: ${elements.pageSizeSelect.value};
  margin: ${margins.top}mm ${margins.right}mm ${margins.bottom}mm ${margins.left}mm;
${marginBoxes}
}
@media screen {
  .document {
    width: ${pageDimensions[0]}mm;
    min-height: ${pageDimensions[1]}mm;
    padding: ${margins.top}mm ${margins.right}mm ${margins.bottom}mm ${margins.left}mm;
  }
  .mdpdf-editable {
    min-height: 120mm;
    outline: none;
    caret-color: ${elements.accentColorInput.value};
  }
  .mdpdf-editable:focus { outline: none; }
  .mdpdf-editable [contenteditable="false"] { cursor: default; user-select: none; }
  .mermaid-rendered {
    position: relative;
    min-height: 52px;
    border: 1px solid transparent;
    border-radius: 6px;
    cursor: pointer !important;
    transition: border-color .15s ease, background .15s ease;
  }
  .mermaid-rendered:hover, .mermaid-rendered:focus {
    border-color: color-mix(in srgb, var(--accent) 38%, transparent);
    background: color-mix(in srgb, var(--accent) 4%, transparent);
    outline: none;
  }
  .mermaid-rendered:not(.mermaid-editing):hover::after,
  .mermaid-rendered:not(.mermaid-editing):focus::after {
    content: "双击编辑 Mermaid";
    position: absolute;
    top: 7px;
    right: 8px;
    padding: 3px 7px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 88%, #000);
    color: #fff;
    font: 11px/1.4 "Segoe UI", sans-serif;
    pointer-events: none;
  }
  .mermaid-rendered.mermaid-editing {
    display: grid;
    gap: 8px;
    padding: 10px;
    border-color: var(--accent);
    background: #f8fbfb;
    cursor: default !important;
  }
  .mermaid-inline-tools { display: flex; justify-content: space-between; gap: 12px; color: #52606b; font: 12px/1.4 "Segoe UI", sans-serif; }
  .mermaid-inline-tools strong { color: var(--ink); }
  .mermaid-inline-source {
    width: 100%;
    min-height: 128px;
    padding: 9px 10px;
    resize: vertical;
    border: 1px solid #b9c5cc;
    border-radius: 5px;
    background: #fff;
    color: #1f2933;
    outline: none;
    user-select: text !important;
    font: 12px/1.55 "Cascadia Code", Consolas, monospace;
    tab-size: 2;
  }
  .mermaid-inline-source:focus { border-color: var(--accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent); }
  .mermaid-inline-preview { min-height: 44px; padding: 8px; overflow: auto; border-radius: 4px; background: #fff; text-align: center; }
  .mermaid-inline-error { padding: 7px 9px; border-radius: 4px; background: #fff0ee; color: #9a342f; font: 12px/1.45 "Cascadia Code", Consolas, monospace; white-space: pre-wrap; user-select: text !important; }
  .mermaid-inline-error[hidden] { display: none; }
  .document::before {
    content: ${elements.headerEnabledToggle.checked ? cssString(screenHeader) : '""'};
    display: ${elements.headerEnabledToggle.checked && screenHeader ? "block" : "none"};
    margin: -${Math.max(0, margins.top - 6)}mm 0 8mm;
    padding-bottom: 3mm;
    border-bottom: 1px solid #d8dde3;
    color: #606a73;
    font-size: 8pt;
    text-align: ${elements.headerAlignSelect.value};
  }
  .document::after {
    content: ${elements.footerEnabledToggle.checked ? cssString(screenFooter) : '""'};
    display: ${elements.footerEnabledToggle.checked && screenFooter ? "block" : "none"};
    margin: 8mm 0 -${Math.max(0, margins.bottom - 6)}mm;
    padding-top: 3mm;
    border-top: 1px solid #d8dde3;
    color: #606a73;
    font-size: 8pt;
    text-align: ${elements.footerAlignSelect.value};
  }
}
@media print {
  .document::before, .document::after { content: none !important; display: none !important; }
}
${custom}`.trim();
}

function renderRequest(content = null, filename = null) {
  return {
    source_path: state.active?.path || null,
    markdown_content: content,
    compare_markdown_content: null,
    filename,
    theme: effectiveTheme(),
    render_mermaid: elements.mermaidToggle.checked,
    strict_mermaid: false,
    format: {
      cover_enabled: elements.coverToggle.checked,
      toc_enabled: elements.tocToggle.checked,
      chapter_page_break: elements.chapterBreakToggle.checked,
      page_size: elements.pageSizeSelect.value,
      margin_top: `${boundedNumber(elements.marginTopInput, STYLE_DEFAULTS.marginTop, 0, 60)}mm`,
      margin_right: `${boundedNumber(elements.marginRightInput, STYLE_DEFAULTS.marginRight, 0, 60)}mm`,
      margin_bottom: `${boundedNumber(elements.marginBottomInput, STYLE_DEFAULTS.marginBottom, 0, 60)}mm`,
      margin_left: `${boundedNumber(elements.marginLeftInput, STYLE_DEFAULTS.marginLeft, 0, 60)}mm`,
      page_numbers: elements.footerEnabledToggle.checked,
      footer_format: elements.footerTextInput.value.trim() || "{page} / {total}",
      footer_align: elements.footerAlignSelect.value,
      header_enabled: elements.headerEnabledToggle.checked,
      header_format: elements.headerTextInput.value.trim(),
      header_align: elements.headerAlignSelect.value,
      custom_css: composeCustomCss(),
    },
  };
}

async function renderPreviewWithWasm(content, filename) {
  await wasmReady;
  const output = renderMarkdownWasm(
    content,
    filename || "document.md",
    effectiveTheme(),
    elements.mermaidToggle.checked,
    undefined,
    elements.coverToggle.checked,
    elements.tocToggle.checked,
    elements.chapterBreakToggle.checked,
    elements.pageSizeSelect.value,
    composeCustomCss(),
  );
  let result;
  try {
    result = {
      html: output.take_html(),
    };
  } finally {
    output.free();
  }
  if (state.active?.path && result.html.includes("<img")) {
    const images = await call("inline_preview_images", {
      request: { sourcePath: state.active.path, html: result.html },
    });
    result.html = images.html;
  }
  return result;
}

async function renderEmbeddedMermaid(html) {
  if (!elements.mermaidToggle.checked) return { html, warnings: [] };
  const documentView = new DOMParser().parseFromString(html, "text/html");
  const diagrams = [...documentView.querySelectorAll(".mermaid")];
  if (!diagrams.length) return { html, warnings: [] };
  if (!mermaid) {
    return { html, warnings: ["内蔵 Mermaid ランタイムを読み込めません"] };
  }

  const warnings = [];
  const sequence = ++previewSequence;
  for (let index = 0; index < diagrams.length; index += 1) {
    const diagram = diagrams[index];
    const source = diagram.textContent || "";
    const rendered = documentView.createElement("div");
    rendered.className = "mermaid-rendered";
    rendered.dataset.mdpdfMermaidSource = source;
    rendered.setAttribute("contenteditable", "false");
    try {
      const id = `mdpdf-mermaid-${sequence}-${index}`;
      const { svg } = await mermaid.render(id, source);
      rendered.innerHTML = svg;
    } catch (error) {
      const message = error?.message || String(error);
      const errorBlock = documentView.createElement("pre");
      errorBlock.className = "diagram-error";
      errorBlock.textContent = `Mermaid: ${message}`;
      rendered.classList.add("mermaid-invalid");
      rendered.append(errorBlock);
      warnings.push(`Mermaid ${index + 1}: ${message}`);
    }
    diagram.replaceWith(rendered);
  }
  return {
    html: `<!doctype html>\n${documentView.documentElement.outerHTML}`,
    warnings,
  };
}

async function renderMermaidEditorPreview(diagram, editor) {
  const revision = ++editor.revision;
  const source = editor.source.value;
  try {
    const id = `mdpdf-mermaid-inline-${++mermaidEditSequence}`;
    const { svg } = await mermaid.render(id, source);
    if (diagram._mdpdfMermaidEditor !== editor || editor.revision !== revision) return false;
    editor.preview.innerHTML = svg;
    editor.error.hidden = true;
    editor.error.textContent = "";
    editor.lastValidSource = source;
    diagram.classList.remove("mermaid-invalid");
    return true;
  } catch (error) {
    if (diagram._mdpdfMermaidEditor !== editor || editor.revision !== revision) return false;
    editor.error.textContent = `Mermaid: ${error?.message || String(error)}`;
    editor.error.hidden = false;
    diagram.classList.add("mermaid-invalid");
    return false;
  }
}

async function finishMermaidEditing(diagram) {
  const editor = diagram._mdpdfMermaidEditor;
  if (!editor || editor.committing) return;
  editor.committing = true;
  clearTimeout(editor.timer);
  editor.source.disabled = true;
  diagram.dataset.mdpdfMermaidSource = editor.source.value;
  const valid = await renderMermaidEditorPreview(diagram, editor);
  if (!valid) {
    editor.committing = false;
    editor.source.disabled = false;
    editor.source.focus();
    return;
  }
  diagram.replaceChildren(...editor.preview.cloneNode(true).childNodes);
  diagram.classList.remove("mermaid-editing", "mermaid-invalid");
  delete diagram._mdpdfMermaidEditor;
  diagram.focus();
  queueVisualSync();
}

function cancelMermaidEditing(diagram) {
  const editor = diagram._mdpdfMermaidEditor;
  if (!editor) return;
  clearTimeout(editor.timer);
  editor.revision += 1;
  diagram.dataset.mdpdfMermaidSource = editor.originalSource;
  diagram.innerHTML = editor.originalHtml;
  diagram.classList.remove("mermaid-editing");
  diagram.classList.toggle("mermaid-invalid", editor.originalInvalid);
  delete diagram._mdpdfMermaidEditor;
  diagram.focus();
  queueVisualSync();
}

export function beginMermaidEditing(diagram, documentView = diagram?.ownerDocument) {
  if (!diagram || !documentView || !mermaid || diagram._mdpdfMermaidEditor) return false;
  const originalSource = diagram.dataset.mdpdfMermaidSource || "";
  const originalHtml = diagram.innerHTML;
  const originalInvalid = diagram.classList.contains("mermaid-invalid");
  const toolbar = documentView.createElement("div");
  toolbar.className = "mermaid-inline-tools";
  const label = documentView.createElement("strong");
  label.textContent = "Mermaid 源码";
  const hint = documentView.createElement("span");
  hint.textContent = "Ctrl+Enter 完成 · Esc 取消";
  toolbar.append(label, hint);

  const source = documentView.createElement("textarea");
  source.className = "mermaid-inline-source";
  source.value = originalSource;
  source.spellcheck = false;
  source.setAttribute("aria-label", "Mermaid 源码");

  const preview = documentView.createElement("div");
  preview.className = "mermaid-inline-preview";
  if (!originalInvalid) preview.innerHTML = originalHtml;
  const error = documentView.createElement("div");
  error.className = "mermaid-inline-error";
  error.hidden = true;

  const editor = {
    source,
    preview,
    error,
    originalSource,
    originalHtml,
    originalInvalid,
    lastValidSource: originalInvalid ? null : originalSource,
    revision: 0,
    timer: 0,
    committing: false,
  };
  diagram._mdpdfMermaidEditor = editor;
  diagram.classList.add("mermaid-editing");
  diagram.replaceChildren(toolbar, source, preview, error);

  source.addEventListener("input", () => {
    diagram.dataset.mdpdfMermaidSource = source.value;
    queueVisualSync();
    clearTimeout(editor.timer);
    editor.timer = window.setTimeout(() => renderMermaidEditorPreview(diagram, editor), 140);
  });
  source.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      cancelMermaidEditing(diagram);
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      finishMermaidEditing(diagram);
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      syncVisualEditor();
      saveActive();
    }
  });
  source.focus();
  source.setSelectionRange(source.value.length, source.value.length);
  if (originalInvalid) renderMermaidEditorPreview(diagram, editor);
  return true;
}

export function activateMermaidEditors(documentView) {
  for (const diagram of documentView?.querySelectorAll(".mermaid-rendered") || []) {
    diagram.tabIndex = 0;
    diagram.setAttribute("role", "button");
    diagram.setAttribute("aria-label", "Mermaid 图表，双击或按 Enter 编辑");
    diagram.title = "双击编辑 Mermaid 图表";
    diagram.addEventListener("dblclick", (event) => {
      event.preventDefault();
      event.stopPropagation();
      beginMermaidEditing(diagram, documentView);
    });
    diagram.addEventListener("keydown", (event) => {
      if (event.target !== diagram || event.key !== "Enter") return;
      event.preventDefault();
      event.stopPropagation();
      beginMermaidEditing(diagram, documentView);
    });
  }
}

async function updatePreview() {
  if (!state.active) return;
  const revision = elements.markdownEditor.value;
  try {
    let result;
    try {
      result = await renderPreviewWithWasm(revision, state.active.filename);
    } catch (wasmError) {
      console.error("WASM preview failed; using native renderer", wasmError);
      result = await call("render_preview", {
        request: renderRequest(revision, state.active.filename),
      });
    }
    if (revision !== elements.markdownEditor.value) return;
    const renderedMermaid = await renderEmbeddedMermaid(result.html);
    if (revision !== elements.markdownEditor.value) return;
    elements.previewFrame.srcdoc = renderedMermaid.html;
  } catch {
    setStatus("プレビューを更新できません", true);
  }
}

function selectedPaths() {
  return [...state.selected];
}

function batchKey() {
  return JSON.stringify([
    selectedPaths().sort(),
    elements.batchFind.value,
    elements.batchReplace.value,
    elements.caseSensitive.checked,
  ]);
}

async function previewBatchReplace() {
  const paths = selectedPaths();
  if (!paths.length) return setStatus("一括処理する文書を選択してください", true);
  if (!elements.batchFind.value) return setStatus("検索文字列を入力してください", true);
  setBusy(true, "置換件数を計算中…");
  try {
    const result = await call("batch_replace", {
      request: batchRequest(paths, true),
    });
    state.lastBatchPreviewKey = batchKey();
    elements.applyReplaceBtn.disabled = result.filesChanged === 0;
    showBatchResult("置換プレビュー", describeBatchResult(result));
  } finally {
    setBusy(false, "準備完了");
  }
}

async function applyBatchReplace() {
  if (state.lastBatchPreviewKey !== batchKey()) {
    return setStatus("条件が変わりました。先に置換件数を再確認してください", true);
  }
  const paths = selectedPaths();
  if (!window.confirm(`${paths.length} 件をバックアップして置換します。続行しますか？`)) return;
  if (!(await flushDirty())) return;
  setBusy(true, "バックアップして置換中…");
  try {
    const result = await call("batch_replace", {
      request: batchRequest(paths, false),
    });
    state.lastBatchPreviewKey = "";
    elements.applyReplaceBtn.disabled = true;
    showBatchResult("置換完了", describeBatchResult(result));
    await refreshWorkspace({ quiet: true });
    if (state.active && paths.includes(state.active.path)) {
      await openDocument(state.active.path, { force: true });
    }
  } finally {
    setBusy(false, "準備完了");
  }
}

function batchRequest(paths, dryRun) {
  return {
    paths,
    find: elements.batchFind.value,
    replace: elements.batchReplace.value,
    caseSensitive: elements.caseSensitive.checked,
    dryRun,
  };
}

function describeBatchResult(result) {
  const lines = [
    `対象: ${result.filesScanned} 件`,
    `変更: ${result.filesChanged} 件 / ${result.replacements} 箇所`,
  ];
  if (result.backupDir) lines.push(`バックアップ: ${result.backupDir}`);
  for (const change of result.changes.slice(0, 200)) {
    lines.push(`  ${change.relativePath}: ${change.replacements} 箇所`);
  }
  if (result.changes.length > 200) lines.push(`  …ほか ${result.changes.length - 200} 件`);
  if (result.failures.length) {
    lines.push("", "失敗:", ...result.failures);
  }
  return lines.join("\n");
}

async function exportSelected() {
  const paths = selectedPaths();
  if (!paths.length) return setStatus("PDF 出力する文書を選択してください", true);
  if (!(await flushDirty())) return;
  const outputDir = await call("choose_export_folder");
  if (!outputDir) return;
  setBusy(true, `${paths.length} 件を順番に PDF 出力中…`);
  try {
    const result = await call("export_documents", {
      request: {
        paths,
        outputDir,
        render: renderRequest(null, null),
      },
    });
    const lines = [
      `成功: ${result.succeeded} 件`,
      `失敗: ${result.failed} 件`,
      `出力先: ${outputDir}`,
      "",
      ...result.files.map((file) => file.error
        ? `NG  ${file.sourcePath}: ${file.error}`
        : `OK  ${file.outputPath}`),
    ];
    showBatchResult("PDF 一括出力", lines.join("\n"));
  } finally {
    setBusy(false, "準備完了");
  }
}

function showBatchResult(title, body) {
  elements.batchResultTitle.textContent = title;
  elements.batchResultBody.textContent = body;
  elements.batchResultDialog.showModal();
}

function setBusy(busy, text) {
  state.busy = busy;
  elements.operationState.textContent = text;
  elements.previewReplaceBtn.disabled = busy;
  elements.exportSelectedBtn.disabled = busy;
  if (busy) elements.applyReplaceBtn.disabled = true;
}

function setStatus(text, isError = false) {
  elements.operationState.textContent = text;
  elements.operationState.classList.toggle("error", isError);
}

function setDirtyState(text, isDirty) {
  elements.dirtyState.textContent = text;
  elements.dirtyState.classList.toggle("dirty", isDirty);
}

function updateEditorStats() {
  const value = elements.markdownEditor.value;
  const lines = value ? value.split("\n").length : 0;
  elements.editorStats.textContent = `${value.length.toLocaleString()} 文字 / ${lines.toLocaleString()} 行`;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}

function applyStyleValues(value = {}) {
  elements.themeSelect.value = value.theme || STYLE_DEFAULTS.theme;
  if (!elements.themeSelect.value) elements.themeSelect.value = STYLE_DEFAULTS.theme;
  elements.fontFamilySelect.value = value.fontFamily || STYLE_DEFAULTS.fontFamily;
  elements.fontSizeInput.value = value.fontSize ?? STYLE_DEFAULTS.fontSize;
  elements.lineHeightInput.value = value.lineHeight ?? STYLE_DEFAULTS.lineHeight;
  elements.textColorInput.value = value.textColor || STYLE_DEFAULTS.textColor;
  elements.accentColorInput.value = value.accentColor || STYLE_DEFAULTS.accentColor;
  elements.marginTopInput.value = value.marginTop ?? STYLE_DEFAULTS.marginTop;
  elements.marginRightInput.value = value.marginRight ?? STYLE_DEFAULTS.marginRight;
  elements.marginBottomInput.value = value.marginBottom ?? STYLE_DEFAULTS.marginBottom;
  elements.marginLeftInput.value = value.marginLeft ?? STYLE_DEFAULTS.marginLeft;
  elements.headerEnabledToggle.checked = value.headerEnabled ?? STYLE_DEFAULTS.headerEnabled;
  elements.headerTextInput.value = value.headerText ?? STYLE_DEFAULTS.headerText;
  elements.headerAlignSelect.value = value.headerAlign || STYLE_DEFAULTS.headerAlign;
  elements.footerEnabledToggle.checked = value.footerEnabled ?? STYLE_DEFAULTS.footerEnabled;
  elements.footerTextInput.value = value.footerText ?? STYLE_DEFAULTS.footerText;
  elements.footerAlignSelect.value = value.footerAlign || STYLE_DEFAULTS.footerAlign;
  elements.customCssInput.value = value.customCss ?? STYLE_DEFAULTS.customCss;
}

function loadSettings() {
  try {
    const value = JSON.parse(localStorage.getItem("mdpdf-desktop-settings") || "{}");
    applyStyleValues(value);
    elements.pageSizeSelect.value = value.pageSize || "A4";
    elements.mermaidToggle.checked = value.mermaid === true;
    elements.coverToggle.checked = value.cover === true;
    elements.tocToggle.checked = value.toc === true;
    elements.chapterBreakToggle.checked = value.chapterBreak === true;
  } catch {
    localStorage.removeItem("mdpdf-desktop-settings");
    applyStyleValues();
  }
}

function saveSettings() {
  localStorage.setItem("mdpdf-desktop-settings", JSON.stringify({
    theme: elements.themeSelect.value,
    fontFamily: elements.fontFamilySelect.value,
    fontSize: boundedNumber(elements.fontSizeInput, STYLE_DEFAULTS.fontSize, 8, 24),
    lineHeight: boundedNumber(elements.lineHeightInput, STYLE_DEFAULTS.lineHeight, 1, 2.5),
    textColor: elements.textColorInput.value,
    accentColor: elements.accentColorInput.value,
    marginTop: boundedNumber(elements.marginTopInput, STYLE_DEFAULTS.marginTop, 0, 60),
    marginRight: boundedNumber(elements.marginRightInput, STYLE_DEFAULTS.marginRight, 0, 60),
    marginBottom: boundedNumber(elements.marginBottomInput, STYLE_DEFAULTS.marginBottom, 0, 60),
    marginLeft: boundedNumber(elements.marginLeftInput, STYLE_DEFAULTS.marginLeft, 0, 60),
    headerEnabled: elements.headerEnabledToggle.checked,
    headerText: elements.headerTextInput.value,
    headerAlign: elements.headerAlignSelect.value,
    footerEnabled: elements.footerEnabledToggle.checked,
    footerText: elements.footerTextInput.value,
    footerAlign: elements.footerAlignSelect.value,
    customCss: elements.customCssInput.value,
    pageSize: elements.pageSizeSelect.value,
    mermaid: elements.mermaidToggle.checked,
    cover: elements.coverToggle.checked,
    toc: elements.tocToggle.checked,
    chapterBreak: elements.chapterBreakToggle.checked,
  }));
  clearTimeout(previewTimer);
  previewTimer = window.setTimeout(updatePreview, 120);
}

function escapeMarkdownText(value) {
  return String(value).replaceAll("\u00a0", " ").replace(/([\\`*_[\]])/g, "\\$1");
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function inlineNodeToMarkdown(node) {
  if (node.nodeType === Node.TEXT_NODE) return escapeMarkdownText(node.nodeValue || "");
  if (node.nodeType !== Node.ELEMENT_NODE) return "";
  const element = node;
  const tag = element.tagName.toLowerCase();
  const inner = () => [...element.childNodes].map(inlineNodeToMarkdown).join("");

  if (tag === "br") return "\n";
  if (tag === "strong" || tag === "b") return `**${inner()}**`;
  if (tag === "em" || tag === "i") return `*${inner()}*`;
  if (tag === "del" || tag === "s" || tag === "strike") return `~~${inner()}~~`;
  if (tag === "code") return `\`${(element.textContent || "").replaceAll("`", "\\`")}\``;
  if (tag === "a") {
    if (element.classList.contains("anchor") && !element.textContent) return "";
    return `[${inner() || escapeMarkdownText(element.getAttribute("href") || "链接")}](${element.getAttribute("href") || ""})`;
  }
  if (tag === "img") {
    const source = element.dataset.mdpdfSrc || element.getAttribute("src") || "";
    return `![${escapeMarkdownText(element.getAttribute("alt") || "图片")}](${source})`;
  }
  if (tag === "input") return "";
  return inner();
}

function listItemToMarkdown(item, ordered, index, depth) {
  const checkbox = [...item.children].find((child) => child.matches?.('input[type="checkbox"]'));
  const inline = [...item.childNodes]
    .filter((child) => !(child.nodeType === Node.ELEMENT_NODE && ["ul", "ol"].includes(child.tagName.toLowerCase())))
    .map(inlineNodeToMarkdown)
    .join("")
    .trim();
  const marker = checkbox ? `- [${checkbox.checked ? "x" : " "}] ` : ordered ? `${index + 1}. ` : "- ";
  let output = `${"  ".repeat(depth)}${marker}${inline}\n`;
  for (const nested of [...item.children].filter((child) => ["ul", "ol"].includes(child.tagName.toLowerCase()))) {
    output += listToMarkdown(nested, depth + 1);
  }
  return output;
}

function listToMarkdown(list, depth = 0) {
  const ordered = list.tagName.toLowerCase() === "ol";
  return [...list.children]
    .filter((child) => child.tagName.toLowerCase() === "li")
    .map((item, index) => listItemToMarkdown(item, ordered, index, depth))
    .join("");
}

function tableToMarkdown(table) {
  const rows = [...table.querySelectorAll("tr")].map((row) =>
    [...row.children].filter((cell) => ["th", "td"].includes(cell.tagName.toLowerCase()))
      .map((cell) => inlineNodeToMarkdown(cell).replaceAll("|", "\\|").trim()));
  if (!rows.length || !rows[0].length) return "";
  const width = Math.max(...rows.map((row) => row.length));
  const normalized = rows.map((row) => [...row, ...Array(Math.max(0, width - row.length)).fill("")]);
  const header = normalized[0];
  return `${[header, Array(width).fill("---"), ...normalized.slice(1)]
    .map((row) => `| ${row.join(" | ")} |`).join("\n")}\n\n`;
}

function blockNodeToMarkdown(node) {
  if (node.nodeType === Node.TEXT_NODE) return escapeMarkdownText(node.nodeValue || "");
  if (node.nodeType !== Node.ELEMENT_NODE) return "";
  const element = node;
  const tag = element.tagName.toLowerCase();
  const inline = () => [...element.childNodes].map(inlineNodeToMarkdown).join("").trim();
  const blocks = () => [...element.childNodes].map(blockNodeToMarkdown).join("");

  if (element.classList.contains("mermaid-rendered")) {
    return `\`\`\`mermaid\n${element.dataset.mdpdfMermaidSource || ""}\n\`\`\`\n\n`;
  }
  if (tag.match(/^h[1-6]$/)) return `${"#".repeat(Number(tag[1]))} ${inline()}\n\n`;
  if (tag === "p") return `${inline()}\n\n`;
  if (tag === "ul" || tag === "ol") return `${listToMarkdown(element)}\n`;
  if (tag === "blockquote") {
    const content = blocks().trim();
    return `${content.split("\n").map((line) => `> ${line}`).join("\n")}\n\n`;
  }
  if (tag === "pre") {
    const language = element.dataset.language || element.querySelector("code")?.className.match(/language-([\w-]+)/)?.[1] || "";
    const source = element.textContent || "";
    return `\`\`\`${language}\n${source.replace(/\n$/, "")}\n\`\`\`\n\n`;
  }
  if (tag === "table") return tableToMarkdown(element);
  if (tag === "hr") return "---\n\n";
  if (tag === "figure") return `${[...element.childNodes].map(blockNodeToMarkdown).join("").trim()}\n\n`;
  if (["section", "article", "main"].includes(tag)) return blocks();
  if (tag === "div") {
    if (element.classList.contains("mermaid")) return `\`\`\`mermaid\n${element.textContent || ""}\n\`\`\`\n\n`;
    return `${blocks() || inline()}\n\n`;
  }
  return inlineNodeToMarkdown(element);
}

export function editableHtmlToMarkdown(editable) {
  const markdown = [...editable.childNodes].map(blockNodeToMarkdown).join("");
  return `${markdown.replace(/\n{3,}/g, "\n\n").trimEnd()}\n`;
}

function syncVisualEditor() {
  const editable = elements.previewFrame.contentDocument?.querySelector(".mdpdf-editable");
  if (!editable || !state.active) return;
  const markdown = editableHtmlToMarkdown(editable);
  if (markdown === elements.markdownEditor.value) return;
  elements.markdownEditor.value = markdown;
  markDirty(false);
}

function queueVisualSync() {
  clearTimeout(visualSyncTimer);
  visualSyncTimer = window.setTimeout(syncVisualEditor, 50);
}

function visualEditorContext() {
  const frameWindow = elements.previewFrame.contentWindow;
  const documentView = elements.previewFrame.contentDocument;
  const editable = documentView?.querySelector(".mdpdf-editable");
  if (!frameWindow || !documentView || !editable) return null;
  const selection = frameWindow.getSelection();
  if (!selection?.rangeCount || !editable.contains(selection.anchorNode)) {
    editable.focus();
  }
  return { frameWindow, documentView, editable };
}

function insertVisualHtml(context, html) {
  context.documentView.execCommand("insertHTML", false, html);
  queueVisualSync();
}

function wrapVisualSelection(context, tag, placeholder) {
  const selection = context.frameWindow.getSelection();
  if (!selection?.rangeCount) return;
  const range = selection.getRangeAt(0);
  const wrapper = context.documentView.createElement(tag);
  if (range.collapsed) wrapper.textContent = placeholder;
  else wrapper.append(range.extractContents());
  range.insertNode(wrapper);
  range.selectNodeContents(wrapper);
  selection.removeAllRanges();
  selection.addRange(range);
  queueVisualSync();
}

function applyMarkdownFormat(action) {
  const context = visualEditorContext();
  if (!context) return;
  context.frameWindow.focus();
  const command = (name, value = null) => context.documentView.execCommand(name, false, value);
  switch (action) {
    case "heading1": command("formatBlock", "h1"); break;
    case "heading2": command("formatBlock", "h2"); break;
    case "bold": command("bold"); break;
    case "italic": command("italic"); break;
    case "strike": command("strikeThrough"); break;
    case "inline-code": wrapVisualSelection(context, "code", "code"); return;
    case "quote": command("formatBlock", "blockquote"); break;
    case "unordered-list": command("insertUnorderedList"); break;
    case "ordered-list": command("insertOrderedList"); break;
    case "task": insertVisualHtml(context, '<ul><li><input type="checkbox"> 任务</li></ul>'); return;
    case "link": {
      const url = window.prompt("链接地址", "https://");
      if (url) command("createLink", url);
      break;
    }
    case "image": {
      const source = window.prompt("工作区内的图片路径", "image.png");
      if (source) {
        insertVisualHtml(context, `<img src="${escapeHtml(source)}" data-mdpdf-src="${escapeHtml(source)}" alt="图片">`);
        window.setTimeout(() => { syncVisualEditor(); updatePreview(); }, 0);
      }
      return;
    }
    case "code-block": insertVisualHtml(context, '<pre data-language="text"><code>代码内容</code></pre>'); return;
    case "table": insertVisualHtml(context, '<table><thead><tr><th>列一</th><th>列二</th><th>列三</th></tr></thead><tbody><tr><td>内容</td><td>内容</td><td>内容</td></tr></tbody></table>'); return;
    case "mermaid":
      elements.mermaidToggle.checked = true;
      insertVisualHtml(context, '<pre data-language="mermaid"><code>graph TD\n  A[开始] --&gt; B[结束]</code></pre>');
      saveSettings();
      window.setTimeout(() => { syncVisualEditor(); updatePreview(); }, 0);
      return;
    case "horizontal-rule": command("insertHorizontalRule"); break;
  }
  queueVisualSync();
}

function activateVisualEditor() {
  const documentView = elements.previewFrame.contentDocument;
  const editable = documentView?.querySelector(".mdpdf-editable");
  if (!editable || !state.active) return;
  editable.contentEditable = "true";
  editable.spellcheck = true;
  editable.setAttribute("aria-label", "直接编辑文档内容");
  activateMermaidEditors(documentView);
  editable.addEventListener("input", queueVisualSync);
  editable.addEventListener("paste", (event) => {
    event.preventDefault();
    const text = event.clipboardData?.getData("text/plain") || "";
    documentView.execCommand("insertText", false, text);
  });
  editable.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey)) return;
    const shortcut = event.key.toLowerCase();
    if (!["b", "i", "k", "s"].includes(shortcut)) return;
    event.preventDefault();
    if (shortcut === "s") saveActive();
    else applyMarkdownFormat(shortcut === "b" ? "bold" : shortcut === "i" ? "italic" : "link");
  });
}

elements.openFolderBtn.addEventListener("click", chooseWorkspace);
elements.refreshBtn.addEventListener("click", () => refreshWorkspace());
elements.fileFilter.addEventListener("input", () => {
  state.filter = elements.fileFilter.value;
  renderFileList();
});
elements.selectAllFiles.addEventListener("change", () => {
  for (const doc of visibleDocuments()) {
    if (elements.selectAllFiles.checked) state.selected.add(doc.path);
    else state.selected.delete(doc.path);
  }
  state.lastBatchPreviewKey = "";
  elements.applyReplaceBtn.disabled = true;
  renderFileList();
});
elements.markdownEditor.addEventListener("input", () => markDirty(true));
elements.previewFrame.addEventListener("load", activateVisualEditor);
elements.saveBtn.addEventListener("click", saveActive);
elements.reloadBtn.addEventListener("click", async () => {
  if (!state.active) return;
  if (state.active.dirty && !window.confirm("未保存の変更を破棄して再読み込みしますか？")) return;
  await openDocument(state.active.path, { force: true });
});
elements.settingsToggle.addEventListener("click", () => {
  elements.settingsPanel.hidden = !elements.settingsPanel.hidden;
});
const settingControls = [
  elements.themeSelect, elements.pageSizeSelect, elements.mermaidToggle,
  elements.coverToggle, elements.tocToggle, elements.chapterBreakToggle,
  elements.fontFamilySelect, elements.fontSizeInput, elements.lineHeightInput,
  elements.textColorInput, elements.accentColorInput, elements.marginTopInput,
  elements.marginRightInput, elements.marginBottomInput, elements.marginLeftInput,
  elements.headerEnabledToggle, elements.headerTextInput, elements.headerAlignSelect,
  elements.footerEnabledToggle, elements.footerTextInput, elements.footerAlignSelect,
  elements.customCssInput,
];
for (const control of settingControls) {
  control.addEventListener("input", saveSettings);
  control.addEventListener("change", saveSettings);
}
elements.resetStyleBtn.addEventListener("click", () => {
  applyStyleValues();
  elements.pageSizeSelect.value = "A4";
  saveSettings();
});
elements.formatToolbar.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-format]");
  if (button) applyMarkdownFormat(button.dataset.format);
});
elements.formatToolbar.addEventListener("pointerdown", (event) => {
  if (event.target.closest("button[data-format]")) event.preventDefault();
});
for (const control of [elements.batchFind, elements.batchReplace, elements.caseSensitive]) {
  control.addEventListener("input", () => {
    state.lastBatchPreviewKey = "";
    elements.applyReplaceBtn.disabled = true;
  });
  control.addEventListener("change", () => {
    state.lastBatchPreviewKey = "";
    elements.applyReplaceBtn.disabled = true;
  });
}
elements.previewReplaceBtn.addEventListener("click", previewBatchReplace);
elements.applyReplaceBtn.addEventListener("click", applyBatchReplace);
elements.exportSelectedBtn.addEventListener("click", exportSelected);
document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveActive();
  }
});
window.addEventListener("beforeunload", (event) => {
  if (state.active?.dirty) event.preventDefault();
});

loadSettings();
window.setInterval(() => refreshWorkspace({ quiet: true }).catch(() => {}), 3000);
