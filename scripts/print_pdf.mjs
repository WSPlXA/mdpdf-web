import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [, , htmlPathArg, pdfPathArg, optionsPathArg] = process.argv;

if (!htmlPathArg || !pdfPathArg || !optionsPathArg) {
  console.error("usage: node scripts/print_pdf.mjs <html-path> <pdf-path> <options-json>");
  process.exit(2);
}

const htmlPath = resolve(htmlPathArg);
const pdfPath = resolve(pdfPathArg);
const optionsPath = resolve(optionsPathArg);
const chromium = process.env.MDPDF_CHROMIUM || "chromium";
const printOptions = await readPrintOptions(optionsPath);

const chrome = spawn(
  chromium,
  [
    "--headless=new",
    "--disable-gpu",
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--remote-debugging-port=0",
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);

const endpoint = await readDevToolsEndpoint(chrome);
const cdp = await connectCdp(endpoint);

try {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", {
    targetId,
    flatten: true,
  });

  await cdp.send("Page.enable", {}, sessionId);
  const loaded = cdp.waitFor("Page.loadEventFired", 30_000, sessionId);
  await cdp.send("Page.navigate", { url: pathToFileURL(htmlPath).href }, sessionId);
  await loaded;

  const result = await cdp.send(
    "Page.printToPDF",
    {
      printBackground: printOptions.printBackground,
      preferCSSPageSize: printOptions.preferCssPageSize,
      displayHeaderFooter: printOptions.displayHeaderFooter,
      headerTemplate: printOptions.headerTemplate,
      footerTemplate: printOptions.footerTemplate,
      marginTop: printOptions.marginTop,
      marginRight: printOptions.marginRight,
      marginBottom: printOptions.marginBottom,
      marginLeft: printOptions.marginLeft,
    },
    sessionId,
  );

  await mkdir(dirname(pdfPath), { recursive: true });
  await writeFile(pdfPath, Buffer.from(result.data, "base64"));
} finally {
  cdp.close();
  chrome.kill("SIGTERM");
}

async function readPrintOptions(path) {
  const raw = JSON.parse(await readFile(path, "utf8"));
  return {
    printBackground: raw.printBackground !== false,
    preferCssPageSize: raw.preferCssPageSize !== false,
    displayHeaderFooter: raw.displayHeaderFooter === true,
    headerTemplate: typeof raw.headerTemplate === "string" ? raw.headerTemplate : "<div></div>",
    footerTemplate: typeof raw.footerTemplate === "string" ? raw.footerTemplate : "<div></div>",
    marginTop: numberOrZero(raw.marginTop),
    marginRight: numberOrZero(raw.marginRight),
    marginBottom: numberOrZero(raw.marginBottom),
    marginLeft: numberOrZero(raw.marginLeft),
  };
}

function numberOrZero(value) {
  return Number.isFinite(value) ? value : 0;
}

function readDevToolsEndpoint(child) {
  return new Promise((resolveEndpoint, reject) => {
    let stderr = "";
    const timer = setTimeout(() => {
      reject(new Error("timed out waiting for DevTools endpoint"));
    }, 10_000);

    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolveEndpoint(match[1]);
      }
    });

    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });

    child.on("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`chromium exited before DevTools endpoint was ready: ${code}`));
    });
  });
}

async function connectCdp(endpoint) {
  if (typeof WebSocket !== "function") {
    throw new Error("Node runtime does not provide global WebSocket");
  }

  const socket = new WebSocket(endpoint);
  let nextId = 1;
  const pending = new Map();
  const waiters = new Map();

  await new Promise((resolveOpen, reject) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", () => reject(new Error("CDP WebSocket failed to open")), {
      once: true,
    });
  });

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);

    if (message.id && pending.has(message.id)) {
      const { resolveMessage, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) {
        reject(new Error(message.error.message || JSON.stringify(message.error)));
      } else {
        resolveMessage(message.result || {});
      }
      return;
    }

    const key = eventKey(message.method, message.sessionId);
    if (message.method && waiters.has(key)) {
      for (const waiter of waiters.get(key)) {
        waiter.resolveEvent(message.params || {});
      }
      waiters.delete(key);
    }
  });

  return {
    send(method, params = {}, sessionId = undefined) {
      const id = nextId++;
      const command = { id, method, params };
      if (sessionId) {
        command.sessionId = sessionId;
      }
      socket.send(JSON.stringify(command));
      return new Promise((resolveMessage, reject) => {
        pending.set(id, { resolveMessage, reject });
      });
    },
    waitFor(method, timeoutMs, sessionId = undefined) {
      return new Promise((resolveEvent, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`timed out waiting for ${method}`));
        }, timeoutMs);
        const wrapped = {
          resolveEvent(value) {
            clearTimeout(timer);
            resolveEvent(value);
          },
        };
        const key = eventKey(method, sessionId);
        const list = waiters.get(key) || [];
        list.push(wrapped);
        waiters.set(key, list);
      });
    },
    close() {
      socket.close();
    },
  };

  function eventKey(method, sessionId = undefined) {
    return `${sessionId || ""}:${method}`;
  }
}
