// Headless test of the Clementine live-drag UI using jsdom and a fake Tauri bridge.
const { JSDOM } = require("jsdom");
const fs = require("fs");
const root = require("path").join(__dirname, "..", "src") + require("path").sep;

const html = fs.readFileSync(root + "index.html", "utf8").replace(/<script[^>]*><\/script>/, "");
const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://localhost/" });
const { window } = dom;

// ---- fake backend mirroring the Rust command contract ---------------------------
const calls = [];
const handlers = {};
let dragHandler;
let tools = [
  { id: "ffmpeg", label: "FFmpeg", found: true, used_for: "Images, audio, video" },
  { id: "pandoc", label: "Pandoc", found: false, used_for: "Documents" },
];
const KIND = { png: "image", jpg: "image", mp3: "audio", wav: "audio", mp4: "video", pdf: "pdf", docx: "document", xyz: "other" };
const TARGETS = {
  image: ["jpg", "webp", "tiff", "avif", "bmp", "gif", "ico", "pdf"],
  audio: ["mp3", "m4a", "wav", "flac", "ogg", "opus"],
  video: ["mp4", "webm", "gif", "mp3"],
  pdf: ["docx", "pptx", "xlsx", "txt", "md", "jpg", "png"],
  document: ["pdf", "md", "txt", "html"],
};
const TOOLS = {
  image: [
    { id: "compress", label: "Compress", title: "Compress", fields: [], multi: false },
    { id: "resize", label: "Resize", title: "Resize image", fields: [{ key: "a", label: "New width (px)", placeholder: "1280" }], multi: false },
  ],
  pdf: [{ id: "compress", label: "Compress", title: "Compress", fields: [], multi: false }],
};
const ext = (p) => (p.split(".").pop() || "").toLowerCase();
let failNext = false;
let folder = "D:\\Backup";

window.__TAURI__ = {
  core: {
    invoke: async (cmd, args = {}) => {
      calls.push([cmd, args]);
      if (failNext && ["convert", "run_tool", "run_multi"].includes(cmd)) {
        failNext = false;
        throw "ffmpeg failed: boom";
      }
      switch (cmd) {
        case "tool_status": return tools;
        case "stats": return { conversions: 519, tools: 20 };
        case "describe": {
          const kinds = [...new Set(args.paths.map((p) => KIND[ext(p)] || "other"))];
          if (kinds.length > 1) return { kind: "mixed", targets: [], tools: [] };
          const k = kinds[0];
          if (k === "other") return { kind: "other", targets: [], tools: [] };
          const t = (TOOLS[k] || []).slice();
          if (args.paths.length > 1 && (k === "pdf" || k === "image")) t.push({ id: "merge", label: k === "pdf" ? "Merge" : "Combine into PDF", title: "Merge", fields: [], multi: true });
          const e0 = ext(args.paths[0]);
          return { kind: k, targets: TARGETS[k].filter((x) => x !== e0), tools: t };
        }
        case "convert": return args.path.replace(/\.\w+$/, "." + args.target);
        case "run_tool": return args.path.replace(/(\.\w+)$/, "-x$1");
        case "run_multi": return args.paths[0].replace(/(\.\w+)$/, "-merged$1");
        case "install_tool": tools = tools.map((t) => (t.id === args.id ? { ...t, found: true } : t)); return "Pandoc installed";
        case "pick_folder": return folder;
        case "file_action": return "Copied to " + args.dest;
        default: return null;
      }
    },
  },
  event: { listen: async (name, cb) => { handlers[name] = cb; return () => {}; } },
  webview: { getCurrentWebview: () => ({ onDragDropEvent: (cb) => (dragHandler = cb) }) },
};
window.innerWidth = 960;
window.innerHeight = 640;
window.navigator.clipboard = { writeText: async (t) => calls.push(["clipboard", t]) };
window.eval(fs.readFileSync(root + "app.js", "utf8"));

// ---- helpers --------------------------------------------------------------------
const $ = (id) => window.document.getElementById(id);
const wait = (ms = 15) => new Promise((r) => setTimeout(r, ms));
let failed = 0, passed = 0;
const check = (name, cond, extra = "") => {
  console.log((cond ? "PASS " : "FAIL ") + name + (cond ? "" : "   -> " + extra));
  cond ? passed++ : failed++;
};
const labels = () => [...$("wheel").querySelectorAll(".item")].map((b) => b.textContent);
const selLabel = () => { const s = $("wheel").querySelector(".item.sel"); return s ? s.textContent : null; };
const activeTab = () => ($("tabs").querySelector(".active") || { textContent: "" }).textContent;
const itemPos = (label) => {
  const el = [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === label);
  return {
    x: parseFloat($("wheel").style.left) + parseFloat(el.style.getPropertyValue("--x")),
    y: parseFloat($("wheel").style.top) + parseFloat(el.style.getPropertyValue("--y")),
  };
};
const modsEvent = (shift, ctrl, alt) => handlers.modifiers({ payload: { shift, ctrl, alt } });
const dd = (type, extra = {}) => dragHandler({ payload: { type, ...extra } });
const enter = (paths, x = 480, y = 320) => dd("enter", { paths, position: { x, y } });
const over = (x, y) => dd("over", { position: { x, y } });
const drop = (paths, x = 480, y = 320) => dd("drop", { paths, position: { x, y } });
const key = (k, extra = {}) => window.dispatchEvent(new window.KeyboardEvent("keydown", { key: k, bubbles: true, ...extra }));
const reset = async () => {
  key("Escape");
  modsEvent(false, false, false);
  dd("leave");
  await wait();
  calls.length = 0;
};
const logText = () => $("log").textContent;

(async () => {
  await wait(30);

  // ---- startup ----
  check("tool pills render and are clickable", $("deps").querySelectorAll("button.pill").length === 2 && $("deps").querySelector(".pill.off"));
  check("stats line shows backend counts", /519 conversion options · 20 file tools/.test($("stats").textContent), $("stats").textContent);
  check("hint names the three keys", /Shift/.test($("hint").textContent) && /Ctrl/.test($("hint").textContent) && /Alt/.test($("hint").textContent));

  // ---- live drag: Shift tapped mid-drag opens the wheel at the cursor ----
  enter(["C:\\pics\\photo.png"], 500, 300);
  check("dragging class set on enter", window.document.body.classList.contains("dragging"));
  check("no wheel before a key is tapped", $("layer").hidden);
  modsEvent(true, false, false);
  await wait();
  check("Shift mid-drag opens the wheel", !$("layer").hidden);
  check("convert ring with real targets, no PNG", labels().join() === "JPG,WEBP,TIFF,AVIF,BMP,GIF,ICO,PDF", labels().join());
  check("wheel centred on the cursor", $("wheel").style.left === "500px" && $("wheel").style.top === "320px" || $("wheel").style.left === "500px", `${$("wheel").style.left},${$("wheel").style.top}`);
  check("nothing highlighted until the cursor is over an item", selLabel() === null);

  // hover by cursor position
  const webp = itemPos("WEBP");
  over(webp.x, webp.y);
  check("cursor over WEBP highlights it", selLabel() === "WEBP", String(selLabel()));
  over(webp.x + 3, webp.y - 4);
  check("small movement keeps it highlighted", selLabel() === "WEBP");
  over(5, 5);
  check("cursor far from items clears highlight", selLabel() === null);

  // switch rings with another key
  modsEvent(false, false, false);
  modsEvent(false, true, false);
  check("tapping Ctrl switches to Tools", activeTab().startsWith("Tools") && labels().join() === "Compress,Resize", activeTab() + " " + labels().join());
  modsEvent(false, false, false);
  modsEvent(false, false, true);
  check("tapping Alt switches to Actions", activeTab().startsWith("Actions") && labels().includes("Copy to…") && labels().includes("Move to…"));
  modsEvent(false, false, false);
  modsEvent(true, false, false);
  check("back to Convert with Shift", activeTab().startsWith("Convert"));

  // drop on an item runs it
  const gif = itemPos("GIF");
  over(gif.x, gif.y);
  drop(["C:\\pics\\photo.png"], gif.x, gif.y);
  await wait(30);
  check("drop on GIF runs convert to gif", calls.some(([c, a]) => c === "convert" && a.path === "C:\\pics\\photo.png" && a.target === "gif"), JSON.stringify(calls.filter(c => c[0] === "convert")));
  check("wheel closes after running", $("layer").hidden);
  check("log shows saved file with Show button", /Saved photo\.gif/.test(logText()) && $("log").querySelector("button"), logText());
  check("dragging class cleared after drop", !window.document.body.classList.contains("dragging"));
  await reset();

  // ---- drop between items keeps the wheel open (click mode) ----
  enter(["C:\\music\\song.wav"], 480, 320);
  modsEvent(true, false, false);
  await wait();
  over(2, 2);
  drop(["C:\\music\\song.wav"], 2, 2);
  await wait();
  check("drop away from items keeps wheel open", !$("layer").hidden);
  check("click mode pre-selects the first item", selLabel() === "MP3", String(selLabel()));
  key("ArrowRight");
  check("arrow keys move the selection", selLabel() === "M4A");
  key("Enter");
  await wait(30);
  check("Enter runs the selected conversion", calls.some(([c, a]) => c === "convert" && a.target === "m4a"));
  await reset();

  // ---- leaving the window cancels ----
  enter(["C:\\pics\\a.png"]);
  modsEvent(true, false, false);
  await wait();
  dd("leave");
  check("leaving the window closes the wheel", $("layer").hidden);
  check("no command ran", calls.filter(([c]) => c === "convert").length === 0);
  await reset();

  // ---- key already held when the files enter ----
  modsEvent(false, true, false);
  enter(["C:\\pics\\a.png"], 300, 300);
  await wait();
  check("Ctrl held at enter opens Tools immediately", !$("layer").hidden && activeTab().startsWith("Tools"));
  await reset();

  // ---- plain drop (no keys) opens the wheel in click mode ----
  drop(["C:\\pics\\a.png"], 400, 300);
  await wait();
  check("plain drop opens Convert ring", !$("layer").hidden && activeTab().startsWith("Convert") && selLabel() === "JPG", activeTab() + selLabel());
  key("Control", { ctrlKey: true });
  check("Ctrl while open switches to Tools", activeTab().startsWith("Tools"));
  await reset();

  // ---- mixed / unknown types only get Actions ----
  enter(["a.png", "b.mp3"]);
  modsEvent(true, false, false);
  await wait();
  check("mixed types fall back to Actions", activeTab().startsWith("Actions") && labels().includes("Open"), activeTab());
  await reset();
  enter(["C:\\x\\weird.xyz"]);
  modsEvent(true, false, false);
  await wait();
  check("unknown type falls back to Actions", activeTab().startsWith("Actions"));
  await reset();

  // ---- tools: parameter prompt ----
  drop(["C:\\pics\\photo.png"]);
  await wait();
  key("Control", { ctrlKey: true });
  $("wheel").querySelectorAll(".item")[1].click(); // Resize
  await wait();
  check("Resize asks for a width", !$("modal").hidden && $("modalTitle").textContent === "Resize image");
  $("modalFields").querySelector("input").value = "800";
  $("modalForm").dispatchEvent(new window.Event("submit", { cancelable: true, bubbles: true }));
  await wait(30);
  check("run_tool receives the value", calls.some(([c, a]) => c === "run_tool" && a.name === "resize" && a.a === "800"));
  check("modal closed", $("modal").hidden);
  await reset();

  drop(["C:\\pics\\photo.png"]);
  await wait();
  key("Control", { ctrlKey: true });
  $("wheel").querySelectorAll(".item")[1].click();
  await wait();
  $("modalCancel").click();
  await wait();
  check("cancelled prompt runs nothing", !calls.some(([c]) => c === "run_tool"));
  await reset();

  // tools without parameters run directly, on every file
  drop(["C:\\a.png", "C:\\b.png"]);
  await wait();
  key("Control", { ctrlKey: true });
  check("merge tool appears for 2 images", labels().includes("Combine into PDF"), labels().join());
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Compress").click();
  await wait(40);
  check("Compress runs once per file", calls.filter(([c, a]) => c === "run_tool" && a.name === "compress").length === 2);
  await reset();

  // ---- multi-file merge runs once ----
  drop(["C:\\a.pdf", "C:\\b.pdf", "C:\\c.pdf"]);
  await wait();
  key("Control", { ctrlKey: true });
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Merge").click();
  await wait(30);
  const multi = calls.filter(([c]) => c === "run_multi");
  check("Merge calls run_multi once with all 3 files", multi.length === 1 && multi[0][1].paths.length === 3, JSON.stringify(multi));
  check("merge result logged", /Saved a-merged\.pdf/.test(logText()), logText());
  await reset();

  // ---- actions ----
  drop(["C:\\docs\\report.docx"]);
  await wait();
  key("Alt", { altKey: true });
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Copy to…").click();
  await wait(30);
  check("Copy to… asks for a folder then copies", calls.some(([c]) => c === "pick_folder") && calls.some(([c, a]) => c === "file_action" && a.name === "copy-to" && a.dest === "D:\\Backup" && a.path === "C:\\docs\\report.docx"), JSON.stringify(calls));
  await reset();

  folder = null;
  drop(["C:\\docs\\report.docx"]);
  await wait();
  key("Alt", { altKey: true });
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Move to…").click();
  await wait(30);
  check("cancelling the folder picker does nothing", !calls.some(([c]) => c === "file_action"));
  folder = "D:\\Backup";
  await reset();

  drop(["C:\\docs\\report.docx", "C:\\docs\\other.docx"]);
  await wait();
  key("Alt", { altKey: true });
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Copy path").click();
  await wait();
  check("Copy path puts all paths on the clipboard", calls.some(([c, t]) => c === "clipboard" && t === "C:\\docs\\report.docx\nC:\\docs\\other.docx"));
  await reset();

  drop(["C:\\docs\\report.docx"]);
  await wait();
  key("Alt", { altKey: true });
  [...$("wheel").querySelectorAll(".item")].find((b) => b.textContent === "Float window").click();
  await wait();
  check("Float window turns on always-on-top", calls.some(([c, a]) => c === "set_float" && a.on === true));
  check("Float button reflects state", $("pinBtn").classList.contains("on") && $("pinBtn").getAttribute("aria-pressed") === "true");
  $("pinBtn").click();
  await wait();
  check("header button turns it off again", calls.filter(([c]) => c === "set_float").pop()[1].on === false && !$("pinBtn").classList.contains("on"));
  await reset();

  // ---- errors surface ----
  failNext = true;
  drop(["C:\\pics\\bad.png"]);
  await wait();
  $("wheel").querySelectorAll(".item")[0].click();
  await wait(30);
  const err = $("log").querySelector("li.err");
  check("backend error shown in the log", !!err && /boom/.test(err.textContent), logText());
  await reset();

  // ---- escape ----
  drop(["C:\\pics\\a.png"]);
  await wait();
  key("Escape");
  check("Esc closes the wheel", $("layer").hidden);

  // ---- settings: remap keys ----
  $("settingsBtn").click();
  check("settings panel opens", !$("panel").hidden && $("keyRows").querySelectorAll("select").length === 3);
  const convSel = $("keyRows").querySelectorAll("select")[0];
  convSel.value = "Ctrl";
  convSel.dispatchEvent(new window.Event("change"));
  const sels = [...$("keyRows").querySelectorAll("select")].map((s) => s.value);
  check("remapping swaps keys so each stays unique", sels.join() === "Ctrl,Shift,Alt", sels.join());
  check("hint updates", $("hint").textContent.indexOf("Ctrl") < $("hint").textContent.indexOf("Shift"), $("hint").textContent);
  check("mapping persists", /Ctrl/.test(window.localStorage.getItem("clementine.keymap")));
  $("panelClose").click();
  enter(["C:\\pics\\a.png"]);
  modsEvent(false, true, false);
  await wait();
  check("Ctrl now opens Convert", !$("layer").hidden && activeTab().startsWith("Convert"), activeTab());
  await reset();

  // ---- helper tool install ----
  calls.length = 0;
  $("deps").querySelector(".pill.off").click();
  await wait(40);
  check("clicking a missing tool installs it", calls.some(([c, a]) => c === "install_tool" && a.id === "pandoc"));
  check("status reloaded afterwards", calls.filter(([c]) => c === "tool_status").length >= 1 && !$("deps").querySelector(".pill.off"));
  check("install result logged", /Pandoc installed/.test(logText()), logText());

  console.log(`\n${passed} passed, ${failed} failed`);
  process.exit(failed ? 1 : 0);
})();
