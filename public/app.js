const fileInput = document.querySelector("#fileInput");
const fileLabel = document.querySelector("#fileLabel");
const fileMeta = document.querySelector("#fileMeta");
const themeSelect = document.querySelector("#themeSelect");
const mermaidToggle = document.querySelector("#mermaidToggle");
const strictToggle = document.querySelector("#strictToggle");
const coverToggle = document.querySelector("#coverToggle");
const tocToggle = document.querySelector("#tocToggle");
const chapterBreakToggle = document.querySelector("#chapterBreakToggle");
const docNameInput = document.querySelector("#docNameInput");
const docCodeInput = document.querySelector("#docCodeInput");
const docDateInput = document.querySelector("#docDateInput");
const versionInput = document.querySelector("#versionInput");
const ownerInput = document.querySelector("#ownerInput");
const applyToCoverBtn = document.querySelector("#applyToCoverBtn");
const pageSizeSelect = document.querySelector("#pageSizeSelect");
const marginTopInput = document.querySelector("#marginTopInput");
const marginRightInput = document.querySelector("#marginRightInput");
const marginBottomInput = document.querySelector("#marginBottomInput");
const marginLeftInput = document.querySelector("#marginLeftInput");
const pageNumberToggle = document.querySelector("#pageNumberToggle");
const footerFormatInput = document.querySelector("#footerFormatInput");
const footerAlignSelect = document.querySelector("#footerAlignSelect");
const headerToggle = document.querySelector("#headerToggle");
const headerFormatInput = document.querySelector("#headerFormatInput");
const headerAlignSelect = document.querySelector("#headerAlignSelect");
const previewBtn = document.querySelector("#previewBtn");
const convertBtn = document.querySelector("#convertBtn");
const previewFrame = document.querySelector("#previewFrame");
const serverState = document.querySelector("#serverState");
const downloadLink = document.querySelector("#downloadLink");

const markdownEditor = document.querySelector("#markdownEditor");
const toggleSidebarBtn = document.querySelector("#toggleSidebarBtn");
const savedIndicator = document.querySelector("#savedIndicator");
const shell = document.querySelector(".shell");

let fileId = null;
let currentJob = null;
let shouldDownload = false;

// Default date formatting helper (YYYY/MM/DD)
function getTodayString() {
  const today = new Date();
  const yyyy = today.getFullYear();
  const mm = String(today.getMonth() + 1).padStart(2, '0');
  const dd = String(today.getDate()).padStart(2, '0');
  return `${yyyy}/${mm}/${dd}`;
}

// Default Japanese Cover Page template
const defaultMarkdown = `# ＜ドキュメント名称＞

## 表紙

日本郵便株式会社
【郵便物等事故申告処理システム】
**＜ドキュメント名称＞**

第1.0版

- 初版 ： 2026年07月31日
- 改版 ：　　　　年　　月　　日

---

## 改版履歴

| 項目             | 内容                         |
| ---------------- | ---------------------------- |
| システム名       | 郵便物等事故申告処理システム |
| ID               | ＜BIPROGYドキュメントID＞    |
| ドキュメント名称 | ＜ドキュメント名称＞         |

### 更新情報

| 項目   | 内容       |
| ------ | ---------- |
| 作成者 | BIPROGY    |
| 作成日 | 2026/07/31 |
| 更新者 |            |
| 更新日 |            |

### 改版履歴

| 版数  | 改版内容 | 更新者  | 更新日     |
| ----- | -------- | ------- | ---------- |
| 01.00 | 初版     | BIPROGY | 2026/07/31 |`;

const templateHeader = "## 表紙";

// Automatically updates the cover page checkbox state based on editor content
function updateCoverToggleState() {
  const currentVal = markdownEditor.value;
  coverToggle.checked = currentVal.includes(templateHeader);
}

function insertCoverTemplate() {
  const currentVal = markdownEditor.value;
  if (!currentVal.includes(templateHeader)) {
    // Determine the document name to dynamically customize template placeholders
    let docName = docNameInput.value.trim() || fileLabel.textContent || "ドキュメント名称";
    if (docName === "Markdownを選択") {
      docName = "ドキュメント名称";
    } else {
      docName = docName.replace(/\.(md|markdown)$/i, "");
    }
    
    // Replace the placeholders with actual document name
    const customizedTemplate = defaultMarkdown.replaceAll("＜ドキュメント名称＞", docName);
    markdownEditor.value = customizedTemplate + "\n\n---\n\n" + currentVal;
  }
}

function removeCoverTemplate() {
  const currentVal = markdownEditor.value;
  if (currentVal.includes(templateHeader)) {
    // Split by the first page break ---
    const parts = currentVal.split(/\n---\n/);
    if (parts.length > 2) {
      // Remove both the Cover page and the Revision History page
      parts.shift();
      parts.shift();
      markdownEditor.value = parts.join("\n---\n").trimStart();
    } else if (parts.length > 1) {
      parts.shift();
      markdownEditor.value = parts.join("\n---\n").trimStart();
    } else {
      if (currentVal.startsWith(defaultMarkdown)) {
        markdownEditor.value = currentVal.substring(defaultMarkdown.length).trimStart();
      }
    }
  }
}

// Write control panel input fields into the Cover Page template inside editor
function applyFieldsToCover() {
  let markdown = markdownEditor.value;
  if (!markdown.includes(templateHeader)) {
    // If cover is not present, insert it first!
    insertCoverTemplate();
    markdown = markdownEditor.value;
  }

  const docName = docNameInput.value.trim() || "ドキュメント名称";
  const docCode = docCodeInput.value.trim() || "＜BIPROGYドキュメントID＞";
  const docDate = docDateInput.value.trim() || getTodayString();
  const version = versionInput.value.trim() || "1.0";
  const owner = ownerInput.value.trim() || "BIPROGY";

  // Convert date (e.g. 2026/07/31) to Japanese format (e.g. 2026年07月31日)
  const dateParts = docDate.split("/");
  let jaDate = docDate;
  if (dateParts.length === 3) {
    jaDate = `${dateParts[0]}年${dateParts[1]}月${dateParts[2]}日`;
  }

  // Split markdown into lines and perform targeted replacements
  const lines = markdown.split("\n");
  const updatedLines = lines.map(line => {
    // 1. First title line
    if (line.trim().startsWith("# ") && !line.trim().startsWith("##")) {
      return `# ${docName}`;
    }
    
    // 2. Bold subtitle line
    if (line.trim().startsWith("**") && line.trim().endsWith("**")) {
      return `**${docName}**`;
    }
    
    // 3. Version line (e.g. 第1.0版)
    if (/^第[0-9a-zA-Z\.-]+版$/.test(line.trim())) {
      return `第${version}版`;
    }
    
    // 4. Cover list date line (初版 ： 2026年07月31日)
    if (line.includes("初版") && line.includes("：") && line.includes("年")) {
      return line.replace(/(-\s*初版\s*：\s*).*/, `$1${jaDate}`);
    }
    
    // 5. Document name row in table
    if (line.includes("| ドキュメント名称 ")) {
      return line.replace(/(\| ドキュメント名称\s*\|\s*)[^|]+(\s*\|)/, `$1${docName}$2`);
    }
    
    // 6. ID row in table
    if (line.includes("| ID ")) {
      return line.replace(/(\| ID\s*\|\s*)[^|]+(\s*\|)/, `$1${docCode}$2`);
    }
    
    // 7. Creator row in table
    if (line.includes("| 作成者 ")) {
      return line.replace(/(\| 作成者\s*\|\s*)[^|]+(\s*\|)/, `$1${owner}$2`);
    }
    
    // 8. Creation date row in table
    if (line.includes("| 作成日 ")) {
      return line.replace(/(\| 作成日\s*\|\s*)[^|]+(\s*\|)/, `$1${docDate}$2`);
    }
    
    // 9. Initial revision row in table (e.g. | 01.00 | 初版 | BIPROGY | 2026/07/31 |)
    if (line.includes("初版") && line.startsWith("|")) {
      let temp = line.replace(/(\| )[0-9.]+(\s*\|\s*初版\s*\|)/, `$1${version}$2`);
      temp = temp.replace(/(\| 初版\s*\|\s*)[^|]+(\s*\|)/, `$1${owner}$2`);
      temp = temp.replace(/(\| 初版\s*\|\s*[^|]+\s*\|\s*)[^|]+(\s*\|)/, `$1${docDate}$2`);
      return temp;
    }
    
    return line;
  });

  markdownEditor.value = updatedLines.join("\n");
  updateCoverToggleState();
}

// Setup functions
function setBusy(text) {
  serverState.textContent = text;
  previewBtn.disabled = true;
  convertBtn.disabled = true;
}

function setReady() {
  serverState.textContent = "準備完了";
  previewBtn.disabled = !fileId && !markdownEditor.value;
  convertBtn.disabled = !fileId && !markdownEditor.value;
}

function debounce(func, wait) {
  let timeout;
  return function(...args) {
    clearTimeout(timeout);
    timeout = setTimeout(() => func.apply(this, args), wait);
  };
}

function saveDraft() {
  const draft = {
    file_id: fileId,
    filename: fileLabel.textContent,
    file_meta: fileMeta.textContent,
    markdown_content: markdownEditor.value,
    theme: themeSelect.value,
    render_mermaid: mermaidToggle.checked,
    strict_mermaid: strictToggle.checked,
    cover_enabled: coverToggle.checked,
    toc_enabled: tocToggle.checked,
    chapter_page_break: chapterBreakToggle.checked,
    doc_name: docNameInput.value,
    doc_code: docCodeInput.value,
    doc_date: docDateInput.value,
    version: versionInput.value,
    owner: ownerInput.value,
    page_size: pageSizeSelect.value,
    margin_top: marginTopInput.value,
    margin_right: marginRightInput.value,
    margin_bottom: marginBottomInput.value,
    margin_left: marginLeftInput.value,
    page_numbers: pageNumberToggle.checked,
    footer_format: footerFormatInput.value,
    footer_align: footerAlignSelect.value,
    header_enabled: headerToggle.checked,
    header_format: headerFormatInput.value,
    header_align: headerAlignSelect.value,
  };
  localStorage.setItem("mdpdf_draft", JSON.stringify(draft));
  
  // Show saved indicator
  savedIndicator.style.opacity = "1";
  if (window.savedIndicatorTimeout) {
    clearTimeout(window.savedIndicatorTimeout);
  }
  window.savedIndicatorTimeout = setTimeout(() => {
    savedIndicator.style.opacity = "0";
  }, 1500);
}

const debouncedSaveDraft = debounce(saveDraft, 500);
const debouncedConvert = debounce(async () => {
  shouldDownload = false;
  await convert();
}, 1000);

previewBtn.disabled = true;
convertBtn.disabled = true;

// Register event listeners
fileInput.addEventListener("change", async () => {
  const file = fileInput.files?.[0];
  if (!file) return;
  await uploadFile(file);
});

previewBtn.addEventListener("click", async () => {
  shouldDownload = false;
  await convert();
});

convertBtn.addEventListener("click", async () => {
  shouldDownload = true;
  await convert();
});

toggleSidebarBtn.addEventListener("click", () => {
  shell.classList.toggle("collapsed-sidebar");
  const isCollapsed = shell.classList.contains("collapsed-sidebar");
  toggleSidebarBtn.textContent = isCollapsed ? "⚙️ 設定を展開" : "⚙️ 設定を非表示";
});

// Register draft saving, automatic compilation, and state synchronization listeners
markdownEditor.addEventListener("input", () => {
  updateCoverToggleState();
  debouncedSaveDraft();
  debouncedConvert();
});

const configElements = [
  themeSelect, mermaidToggle, strictToggle, tocToggle,
  chapterBreakToggle, docNameInput, docCodeInput, docDateInput, versionInput, ownerInput, pageSizeSelect,
  marginTopInput, marginRightInput, marginBottomInput, marginLeftInput, pageNumberToggle,
  footerFormatInput, footerAlignSelect, headerToggle, headerFormatInput, headerAlignSelect
];

// Config elements change listeners (excluding coverToggle)
configElements.forEach(elem => {
  if (elem) {
    if (elem.type === "checkbox" || elem.tagName === "SELECT") {
      elem.addEventListener("change", async () => {
        saveDraft();
        shouldDownload = false;
        await convert();
      });
    } else {
      elem.addEventListener("input", () => {
        debouncedSaveDraft();
        debouncedConvert();
      });
    }
  }
});

// Separate listener for coverToggle to handle template injection
coverToggle.addEventListener("change", async () => {
  if (coverToggle.checked) {
    insertCoverTemplate();
  } else {
    removeCoverTemplate();
  }
  saveDraft();
  shouldDownload = false;
  await convert();
});

// One-click write sidebar fields into cover page template in the editor
applyToCoverBtn.addEventListener("click", async () => {
  applyFieldsToCover();
  saveDraft();
  shouldDownload = false;
  await convert();
});

// Register paste event for clipboard images
markdownEditor.addEventListener("paste", async (e) => {
  const items = e.clipboardData?.items;
  if (!items) return;
  
  for (const item of items) {
    if (item.type.startsWith("image/")) {
      e.preventDefault(); // Stop normal text pasting
      const file = item.getAsFile();
      if (!file) continue;
      
      setBusy("画像を処理中...");
      const reader = new FileReader();
      reader.onload = async (event) => {
        const base64Url = event.target.result;
        const markdownImage = `\n![image](${base64Url})\n`;
        insertTextAtCursor(markdownEditor, markdownImage);
        saveDraft();
        await convert();
      };
      reader.readAsDataURL(file);
      break;
    }
  }
});

// Register drag and drop events for editor textarea
markdownEditor.addEventListener("dragenter", (e) => {
  e.preventDefault();
  markdownEditor.classList.add("drag-over");
});

markdownEditor.addEventListener("dragover", (e) => {
  e.preventDefault();
  markdownEditor.classList.add("drag-over");
});

markdownEditor.addEventListener("dragleave", (e) => {
  e.preventDefault();
  markdownEditor.classList.remove("drag-over");
});

markdownEditor.addEventListener("drop", async (e) => {
  e.preventDefault();
  markdownEditor.classList.remove("drag-over");
  
  const files = e.dataTransfer?.files;
  if (!files || files.length === 0) return;
  
  const file = files[0];
  
  if (file.type.startsWith("image/")) {
    setBusy("画像を処理中...");
    const reader = new FileReader();
    reader.onload = async (event) => {
      const base64Url = event.target.result;
      const markdownImage = `\n![image](${base64Url})\n`;
      insertTextAtCursor(markdownEditor, markdownImage);
      saveDraft();
      await convert();
    };
    reader.readAsDataURL(file);
  } else if (file.name.endsWith(".md") || file.name.endsWith(".markdown")) {
    await uploadFile(file);
  }
});

function insertTextAtCursor(textarea, text) {
  const start = textarea.selectionStart;
  const end = textarea.selectionEnd;
  const val = textarea.value;
  textarea.value = val.substring(0, start) + text + val.substring(end);
  textarea.selectionStart = textarea.selectionEnd = start + text.length;
  textarea.focus();
}

async function loadDraft() {
  const draftStr = localStorage.getItem("mdpdf_draft");
  const defaultDate = getTodayString();
  if (!draftStr) {
    docDateInput.value = defaultDate;
    return;
  }
  try {
    const draft = JSON.parse(draftStr);
    
    // Restore file info
    fileId = draft.file_id || null;
    fileLabel.textContent = draft.filename || "Markdownを選択";
    fileMeta.textContent = draft.file_meta || ".md / .markdown、最大 10 MiB";
    
    // Restore editor content
    markdownEditor.value = draft.markdown_content || "";
    
    // Restore options
    const loadedTheme = draft.theme || "modern-tech";
    const hasThemeOption = Array.from(themeSelect.options).some(o => o.value === loadedTheme);
    themeSelect.value = hasThemeOption ? loadedTheme : (themeSelect.options[0]?.value || "modern-tech");
    mermaidToggle.checked = draft.render_mermaid !== false;
    strictToggle.checked = !!draft.strict_mermaid;
    coverToggle.checked = !!draft.cover_enabled;
    tocToggle.checked = !!draft.toc_enabled;
    chapterBreakToggle.checked = !!draft.chapter_page_break;
    docNameInput.value = draft.doc_name || "";
    docCodeInput.value = draft.doc_code || "";
    docDateInput.value = draft.doc_date || defaultDate;
    versionInput.value = draft.version || "";
    ownerInput.value = draft.owner || "";
    pageSizeSelect.value = draft.page_size || "A4";
    marginTopInput.value = draft.margin_top || "20mm";
    marginRightInput.value = draft.margin_right || "18mm";
    marginBottomInput.value = draft.margin_bottom || "18mm";
    marginLeftInput.value = draft.margin_left || "18mm";
    pageNumberToggle.checked = draft.page_numbers !== false;
    footerFormatInput.value = draft.footer_format || "{page} / {total}";
    footerAlignSelect.value = draft.footer_align || "right";
    headerToggle.checked = !!draft.header_enabled;
    headerFormatInput.value = draft.header_format || "";
    headerAlignSelect.value = draft.header_align || "left";
    
    // Synchronize cover checkbox state with loaded editor content
    updateCoverToggleState();
    
    // Enable/disable actions
    if (fileId || markdownEditor.value) {
      previewBtn.disabled = false;
      convertBtn.disabled = false;
      
      // Auto trigger preview to render PDF on load
      shouldDownload = false;
      await convert();
    }
  } catch (e) {
    console.error("Failed to load draft:", e);
  }
}

async function uploadFile(file) {
  setBusy("アップロード中...");
  downloadLink.hidden = true;
  downloadLink.removeAttribute("href");
  appendLog(`upload ${file.name}`);

  // Auto-extract filename as document name
  const nameWithoutExt = file.name.replace(/\.(md|markdown)$/i, "");
  docNameInput.value = nameWithoutExt;
  docDateInput.value = getTodayString();

  // Read file contents as text and load into editor
  const reader = new FileReader();
  reader.onload = async (e) => {
    markdownEditor.value = e.target.result;
    
    // Sync checkbox state for the uploaded file
    updateCoverToggleState();
    
    // Automatically preview (compile PDF and show in right panel)
    shouldDownload = false;
    await convert();
    saveDraft();
  };
  reader.readAsText(file);

  const form = new FormData();
  form.append("file", file);
  const response = await fetchJson("/api/files", { method: "POST", body: form });
  fileId = response.file_id;
  fileLabel.textContent = response.filename;
  fileMeta.textContent = `${Math.round(response.size / 1024)} KiB`;
  saveDraft();
  setReady();
}

async function convert() {
  if (!fileId && !markdownEditor.value) return;
  setBusy("処理待ち...");
  downloadLink.hidden = true;
  const response = await fetchJson("/api/convert", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(renderPayload()),
  });
  currentJob = response.job_id;
  appendLog(`job ${currentJob} queued`);
  pollJob();
}

async function pollJob() {
  if (!currentJob) return;
  const job = await fetchJson(`/api/jobs/${currentJob}`);
  appendLogs(job.logs.slice(-2));

  if (job.status === "succeeded") {
    downloadLink.href = job.pdf_url;
    downloadLink.hidden = false;
    previewFrame.removeAttribute("srcdoc");
    previewFrame.src = job.pdf_url + "?inline=true&t=" + Date.now();
    
    if (shouldDownload) {
      const filename = fileLabel.textContent !== "Markdownを選択" ? fileLabel.textContent.replace(/\.md$/i, ".pdf") : "document.pdf";
      triggerDownload(job.pdf_url, filename);
    }
    
    appendLogs(job.warnings.map((item) => `warning: ${item}`));
    setReady();
    return;
  }

  if (job.status === "failed") {
    appendLog(job.error_message || "job failed", true);
    setReady();
    return;
  }

  window.setTimeout(pollJob, 1000);
}

function triggerDownload(url, filename) {
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

function renderPayload() {
  return {
    file_id: fileId,
    markdown_content: markdownEditor.value,
    filename: fileLabel.textContent !== "Markdownを選択" ? fileLabel.textContent : "document.md",
    theme: themeSelect.value,
    render_mermaid: mermaidToggle.checked,
    strict_mermaid: strictToggle.checked,
    format: formatPayload(),
  };
}

function formatPayload() {
  return compactObject({
    cover_enabled: coverToggle.checked,
    toc_enabled: tocToggle.checked,
    chapter_page_break: chapterBreakToggle.checked,
    doc_name: cleanValue(docNameInput.value),
    doc_code: cleanValue(docCodeInput.value),
    doc_date: cleanValue(docDateInput.value),
    version: cleanValue(versionInput.value),
    owner: cleanValue(ownerInput.value),
    page_size: cleanValue(pageSizeSelect.value),
    margin_top: cleanValue(marginTopInput.value),
    margin_right: cleanValue(marginRightInput.value),
    margin_bottom: cleanValue(marginBottomInput.value),
    margin_left: cleanValue(marginLeftInput.value),
    page_numbers: pageNumberToggle.checked,
    footer_format: cleanValue(footerFormatInput.value),
    footer_align: cleanValue(footerAlignSelect.value),
    header_enabled: headerToggle.checked,
    header_format: cleanValue(headerFormatInput.value),
    header_align: cleanValue(headerAlignSelect.value),
  });
}

function compactObject(source) {
  const result = {};
  for (const [key, value] of Object.entries(source)) {
    if (value !== "" && value !== null && value !== undefined) result[key] = value;
  }
  return result;
}

function cleanValue(value) {
  return `${value ?? ""}`.trim();
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, options);
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    const message = payload.error || `${response.status} ${response.statusText}`;
    appendLog(message, true);
    setReady();
    throw new Error(message);
  }
  return payload;
}

function appendLogs(items) {
  for (const item of items || []) appendLog(item);
}

function appendLog(text, error = false) {
  if (error) {
    console.error(text);
    serverState.textContent = text;
    serverState.style.color = "var(--danger)";
  } else {
    console.log(text);
    serverState.style.color = "";
  }
}

// Initial call to load saved draft on page load
loadDraft();
