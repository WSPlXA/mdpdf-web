const fileInput = document.querySelector("#fileInput");
const fileLabel = document.querySelector("#fileLabel");
const fileMeta = document.querySelector("#fileMeta");
const themeSelect = document.querySelector("#themeSelect");
const mermaidToggle = document.querySelector("#mermaidToggle");
const strictToggle = document.querySelector("#strictToggle");
const coverToggle = document.querySelector("#coverToggle");
const tocToggle = document.querySelector("#tocToggle");
const chapterBreakToggle = document.querySelector("#chapterBreakToggle");
const docCodeInput = document.querySelector("#docCodeInput");
const versionInput = document.querySelector("#versionInput");
const ownerInput = document.querySelector("#ownerInput");
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
const shell = document.querySelector(".shell");

let fileId = null;
let currentJob = null;
let shouldDownload = false;

previewBtn.disabled = true;
convertBtn.disabled = true;

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
  toggleSidebarBtn.textContent = isCollapsed ? "⚙️ 展开设置" : "⚙️ 收起设置";
});

async function uploadFile(file) {
  setBusy("uploading");
  downloadLink.hidden = true;
  downloadLink.removeAttribute("href");
  appendLog(`upload ${file.name}`);

  // Read file contents as text and load into editor
  const reader = new FileReader();
  reader.onload = async (e) => {
    markdownEditor.value = e.target.result;
    
    // Automatically preview (compile PDF and show in right panel)
    shouldDownload = false;
    await convert();
  };
  reader.readAsText(file);

  const form = new FormData();
  form.append("file", file);
  const response = await fetchJson("/api/files", { method: "POST", body: form });
  fileId = response.file_id;
  fileLabel.textContent = response.filename;
  fileMeta.textContent = `${Math.round(response.size / 1024)} KiB`;
  setReady();
}

async function convert() {
  if (!fileId && !markdownEditor.value) return;
  setBusy("queued");
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
      const filename = fileLabel.textContent !== "选择 Markdown" ? fileLabel.textContent.replace(/\.md$/i, ".pdf") : "document.pdf";
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
    filename: fileLabel.textContent !== "选择 Markdown" ? fileLabel.textContent : "document.md",
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
    doc_code: cleanValue(docCodeInput.value),
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

function setBusy(text) {
  serverState.textContent = text;
  previewBtn.disabled = true;
  convertBtn.disabled = true;
}

function setReady() {
  serverState.textContent = "ready";
  previewBtn.disabled = !fileId;
  convertBtn.disabled = !fileId;
}
