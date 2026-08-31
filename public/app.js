import initWasm, { render_markdown_fast as renderMarkdownWasm } from "./wasm/mdpdf_wasm.js";

const invoke = window.__TAURI__?.core?.invoke;
const wasmReady = initWasm();

const elements = Object.fromEntries([
  "openFolderBtn", "refreshBtn", "workspacePath", "fileFilter", "selectAllFiles",
  "selectionCount", "fileList", "activeFilename", "activeRelativePath", "dirtyState",
  "reloadBtn", "saveBtn", "markdownEditor", "autoSaveToggle", "editorStats",
  "settingsToggle", "settingsPanel", "themeSelect", "pageSizeSelect",
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

function markDirty() {
  if (!state.active) return;
  state.active.dirty = true;
  setDirtyState("未保存", true);
  updateEditorStats();
  clearTimeout(previewTimer);
  previewTimer = window.setTimeout(updatePreview, 120);
  if (elements.autoSaveToggle.checked) {
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(saveActive, 850);
  }
}

function renderRequest(content = null, filename = null) {
  return {
    source_path: state.active?.path || null,
    markdown_content: content,
    compare_markdown_content: null,
    filename,
    theme: elements.themeSelect.value,
    render_mermaid: elements.mermaidToggle.checked,
    strict_mermaid: false,
    format: {
      cover_enabled: elements.coverToggle.checked,
      toc_enabled: elements.tocToggle.checked,
      chapter_page_break: elements.chapterBreakToggle.checked,
      page_size: elements.pageSizeSelect.value,
      page_numbers: true,
    },
  };
}

async function renderPreviewWithWasm(content, filename) {
  await wasmReady;
  const output = renderMarkdownWasm(
    content,
    filename || "document.md",
    elements.themeSelect.value,
    elements.mermaidToggle.checked,
    undefined,
    elements.coverToggle.checked,
    elements.tocToggle.checked,
    elements.chapterBreakToggle.checked,
    elements.pageSizeSelect.value,
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
    try {
      const id = `mdpdf-mermaid-${sequence}-${index}`;
      const { svg } = await mermaid.render(id, diagram.textContent || "");
      const rendered = documentView.createElement("div");
      rendered.className = "mermaid-rendered";
      rendered.innerHTML = svg;
      diagram.replaceWith(rendered);
    } catch (error) {
      const message = error?.message || String(error);
      const errorBlock = documentView.createElement("pre");
      errorBlock.className = "diagram-error";
      errorBlock.textContent = `Mermaid: ${message}`;
      diagram.replaceWith(errorBlock);
      warnings.push(`Mermaid ${index + 1}: ${message}`);
    }
  }
  return {
    html: `<!doctype html>\n${documentView.documentElement.outerHTML}`,
    warnings,
  };
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

function loadSettings() {
  try {
    const value = JSON.parse(localStorage.getItem("mdpdf-desktop-settings") || "{}");
    elements.themeSelect.value = value.theme || "modern-tech";
    elements.pageSizeSelect.value = value.pageSize || "A4";
    elements.mermaidToggle.checked = value.mermaid === true;
    elements.coverToggle.checked = value.cover === true;
    elements.tocToggle.checked = value.toc === true;
    elements.chapterBreakToggle.checked = value.chapterBreak === true;
  } catch {
    localStorage.removeItem("mdpdf-desktop-settings");
  }
}

function saveSettings() {
  localStorage.setItem("mdpdf-desktop-settings", JSON.stringify({
    theme: elements.themeSelect.value,
    pageSize: elements.pageSizeSelect.value,
    mermaid: elements.mermaidToggle.checked,
    cover: elements.coverToggle.checked,
    toc: elements.tocToggle.checked,
    chapterBreak: elements.chapterBreakToggle.checked,
  }));
  updatePreview();
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
elements.markdownEditor.addEventListener("input", markDirty);
elements.saveBtn.addEventListener("click", saveActive);
elements.reloadBtn.addEventListener("click", async () => {
  if (!state.active) return;
  if (state.active.dirty && !window.confirm("未保存の変更を破棄して再読み込みしますか？")) return;
  await openDocument(state.active.path, { force: true });
});
elements.settingsToggle.addEventListener("click", () => {
  elements.settingsPanel.hidden = !elements.settingsPanel.hidden;
});
for (const control of [
  elements.themeSelect, elements.pageSizeSelect, elements.mermaidToggle,
  elements.coverToggle, elements.tocToggle, elements.chapterBreakToggle,
]) {
  control.addEventListener("change", saveSettings);
}
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
