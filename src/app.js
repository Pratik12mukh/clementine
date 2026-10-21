// Clementine front end. Plain JS, no bundler.
//
// Live drag flow (Windows):
//   1. Drag files over the window  -> "enter" event (paths + position)
//   2. Tap Shift / Ctrl / Alt       -> backend "modifiers" event -> the wheel opens at the cursor
//   3. Move the cursor              -> "over" events highlight the nearest wheel item
//   4. Drop on an item              -> it runs.  Drop anywhere else -> the wheel stays open to click.
// Format lists and tools come from the Rust backend (single source of truth).

const T = window.__TAURI__;
const invoke = T.core.invoke;
const listen = T.event.listen;
const webview = T.webview.getCurrentWebview();

/* ------------------------------------------------------------ constants --- */

const RINGS = [
  { id: "convert", label: "Convert" },
  { id: "tools", label: "Tools" },
  { id: "actions", label: "Actions" },
];
const KEYS = ["Shift", "Ctrl", "Alt"];
const DEFAULT_KEYMAP = { convert: "Shift", tools: "Ctrl", actions: "Alt" };

const ACTIONS = [
  { id: "open", label: "Open" },
  { id: "reveal", label: "Show in Explorer" },
  { id: "copy-path", label: "Copy path" },
  { id: "copy-to", label: "Copy to…", pick: true },
  { id: "move-to", label: "Move to…", pick: true },
  { id: "copy-downloads", label: "Copy to Downloads" },
  { id: "terminal", label: "Terminal here" },
  { id: "float", label: "Float window" },
];

/* ---------------------------------------------------------------- utils --- */

const $ = (id) => document.getElementById(id);
const baseName = (p) => p.split(/[\\/]/).pop();
const store = {
  get(k, d) {
    try {
      const v = localStorage.getItem("clementine." + k);
      return v == null ? d : JSON.parse(v);
    } catch {
      return d;
    }
  },
  set(k, v) {
    try {
      localStorage.setItem("clementine." + k, JSON.stringify(v));
    } catch {}
  },
};

/* ---------------------------------------------------------------- state --- */

let keymap = Object.assign({}, DEFAULT_KEYMAP, store.get("keymap", {}));
if (new Set(Object.values(keymap)).size !== 3) keymap = { ...DEFAULT_KEYMAP };
let floating = !!store.get("float", false);

const mods = { Shift: false, Ctrl: false, Alt: false };
const drag = { active: false, paths: [], desc: null, pos: { x: 0, y: 0 } };
const wheel = {
  open: false,
  mode: "click", // "drag": cursor is still carrying the files; "click": normal menu
  paths: [],
  desc: null,
  ring: "convert",
  items: [],
  sel: -1,
  cx: 0,
  cy: 0,
  radius: 112,
  layout: [], // absolute item centres in CSS px, for hit-testing
};

const ringForKey = (k) => (RINGS.find((r) => keymap[r.id] === k) || {}).id;
const keyForRing = (id) => keymap[id];

/* ----------------------------------------------------------------- log --- */

function log(text, cls, action) {
  const li = document.createElement("li");
  li.className = cls || "";
  const span = document.createElement("span");
  span.textContent = text;
  li.append(span);
  if (action) {
    const b = document.createElement("button");
    b.textContent = action.label;
    b.onclick = action.fn;
    li.append(b);
  }
  const ul = $("log");
  ul.prepend(li);
  while (ul.children.length > 40) ul.lastChild.remove();
  return li;
}

function showButton(path) {
  return {
    label: "Show",
    fn: () => invoke("file_action", { path, name: "reveal", dest: null }).catch((e) => log(String(e), "err")),
  };
}

/* ------------------------------------------------------------- wheel data --- */

function itemsFor(ring, desc) {
  if (ring === "actions") {
    return ACTIONS.map((a) => ({
      ...a,
      type: "action",
      label: a.id === "float" ? (floating ? "Unfloat window" : "Float window") : a.label,
    }));
  }
  if (!desc) return [];
  if (ring === "convert") return desc.targets.map((t) => ({ id: t, label: t.toUpperCase(), type: "convert" }));
  return desc.tools.map((t) => ({ ...t, type: t.multi ? "multi" : "tool" }));
}

function describeFiles(paths) {
  return invoke("describe", { paths }).catch(() => ({ kind: "other", targets: [], tools: [] }));
}

/* ---------------------------------------------------------------- wheel UI --- */

function geometry(n) {
  // Keep neighbouring items at least ~74px apart.
  return Math.max(112, Math.ceil((n * 74) / (2 * Math.PI)));
}

function renderWheel() {
  const el = $("wheel");
  el.style.left = wheel.cx + "px";
  el.style.top = wheel.cy + "px";
  el.replaceChildren();
  wheel.layout = [];

  const hub = document.createElement("div");
  hub.className = "hub";
  const n = document.createElement("div");
  n.className = "name";
  n.textContent = wheel.paths.length > 1 ? `${wheel.paths.length} files` : baseName(wheel.paths[0]);
  const m = document.createElement("div");
  m.className = "meta";
  m.textContent = RINGS.find((r) => r.id === wheel.ring).label;
  hub.append(n, m);
  el.append(hub);

  const items = wheel.items;
  wheel.radius = geometry(items.length);
  if (!items.length) {
    const e = document.createElement("div");
    e.className = "empty";
    e.textContent =
      wheel.ring === "convert"
        ? wheel.desc && wheel.desc.kind === "mixed"
          ? "Mixed file types: only Actions apply."
          : "Nothing to convert this to yet. Try Actions."
        : "No tools for this selection.";
    el.append(e);
  }
  items.forEach((it, i) => {
    const ang = (-90 + (360 / items.length) * i) * (Math.PI / 180);
    const x = Math.round(Math.cos(ang) * wheel.radius);
    const y = Math.round(Math.sin(ang) * wheel.radius);
    wheel.layout.push({ x: wheel.cx + x, y: wheel.cy + y });
    const b = document.createElement("button");
    b.className = "item" + (i === wheel.sel ? " sel" : "");
    b.style.setProperty("--x", x + "px");
    b.style.setProperty("--y", y + "px");
    b.style.animationDelay = i * 12 + "ms";
    b.textContent = it.label;
    b.setAttribute("role", "menuitem");
    b.onmouseenter = () => wheel.mode === "click" && select(i);
    b.onclick = () => choose(i);
    el.append(b);
  });

  $("tabs").replaceChildren(
    ...RINGS.map((r) => {
      const t = document.createElement("button");
      t.className = r.id === wheel.ring ? "active" : "";
      t.setAttribute("role", "tab");
      t.innerHTML = `${r.label}<b>${keyForRing(r.id)}</b>`;
      t.onclick = () => setRing(r.id);
      return t;
    })
  );
  $("keys").textContent =
    wheel.mode === "drag"
      ? "Drop on an option to run it · tap another key to switch wheels · Esc to cancel"
      : "Click or use the arrow keys + Enter · Esc to cancel";
}

function select(i) {
  wheel.sel = i;
  [...$("wheel").querySelectorAll(".item")].forEach((el, k) => el.classList.toggle("sel", k === i));
}

function setRing(ring) {
  if (wheel.open && ring === wheel.ring && wheel.items.length) return;
  const restricted = wheel.desc && (wheel.desc.kind === "mixed" || wheel.desc.kind === "other");
  wheel.ring = restricted && ring !== "actions" ? "actions" : ring;
  wheel.items = itemsFor(wheel.ring, wheel.desc);
  wheel.sel = wheel.mode === "click" && wheel.items.length ? 0 : -1;
  renderWheel();
}

async function openWheel(paths, descPromise, ring, pos, mode) {
  wheel.paths = paths;
  wheel.mode = mode;
  wheel.open = true;
  wheel.desc = null;
  const desc = await descPromise;
  if (!wheel.open || wheel.paths !== paths) return; // cancelled while describing
  wheel.desc = desc;
  const r = geometry(Math.max(desc.targets.length, desc.tools.length, 8));
  const margin = r + 70;
  const w = window.innerWidth;
  const h = window.innerHeight;
  wheel.cx = Math.round(Math.min(Math.max(pos.x, margin), Math.max(w - margin, margin)));
  wheel.cy = Math.round(Math.min(Math.max(pos.y, margin), Math.max(h - margin, margin)));
  $("layer").hidden = false;
  wheel.ring = null;
  setRing(ring);
}

function closeWheel() {
  wheel.open = false;
  wheel.sel = -1;
  $("layer").hidden = true;
}

function hit(pos) {
  let best = -1;
  let bestD = 44; // px: roughly an item's radius plus slack
  wheel.layout.forEach((c, i) => {
    const d = Math.hypot(pos.x - c.x, pos.y - c.y);
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  });
  return best;
}

/* --------------------------------------------------------------- running --- */

async function choose(i) {
  const it = wheel.items[i];
  if (!it) return;
  const paths = wheel.paths.slice();
  closeWheel();
  if (it.type === "convert") {
    return runEach(paths, `Converting to ${it.label}`, (f) => invoke("convert", { path: f, target: it.id }));
  }
  if (it.type === "multi") {
    const li = log(`${it.label}: ${paths.length} files…`, "run");
    try {
      return done(li, await invoke("run_multi", { paths, name: it.id }));
    } catch (e) {
      return fail(li, String(e));
    }
  }
  if (it.type === "tool") {
    let vals = {};
    if (it.fields && it.fields.length) {
      vals = await ask(it.title || it.label, it.fields);
      if (!vals) return;
    }
    return runEach(paths, it.label, (f) => invoke("run_tool", { path: f, name: it.id, a: vals.a ?? null, b: vals.b ?? null }));
  }
  if (it.type === "action") return runAction(paths, it);
}

function done(li, out) {
  li.className = "ok";
  li.firstChild.textContent = `Saved ${baseName(out)}`;
  const s = showButton(out);
  const b = document.createElement("button");
  b.textContent = s.label;
  b.onclick = s.fn;
  li.append(b);
}

function fail(li, msg) {
  li.className = "err";
  li.firstChild.textContent = msg;
}

async function runEach(paths, label, fn) {
  for (let i = 0; i < paths.length; i++) {
    const f = paths[i];
    const li = log(`${label}: ${baseName(f)}${paths.length > 1 ? ` (${i + 1}/${paths.length})` : ""}…`, "run");
    try {
      done(li, await fn(f));
    } catch (e) {
      fail(li, `${baseName(f)}: ${e}`);
    }
  }
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {}
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.cssText = "position:fixed;opacity:0";
  document.body.append(ta);
  ta.select();
  document.execCommand("copy");
  ta.remove();
}

async function setFloat(on) {
  floating = on;
  store.set("float", on);
  $("pinBtn").setAttribute("aria-pressed", String(on));
  $("pinBtn").classList.toggle("on", on);
  try {
    await invoke("set_float", { on });
  } catch (e) {
    log(String(e), "err");
  }
}

async function runAction(paths, it) {
  if (it.id === "copy-path") {
    await copyText(paths.join("\n"));
    return log(paths.length > 1 ? `Copied ${paths.length} paths` : "Copied path to clipboard", "ok");
  }
  if (it.id === "float") {
    await setFloat(!floating);
    return log(floating ? "Clementine now floats above other windows" : "Clementine no longer floats", "ok");
  }
  let dest = null;
  if (it.pick) {
    try {
      dest = await invoke("pick_folder");
    } catch (e) {
      return log(String(e), "err");
    }
    if (!dest) return;
  }
  for (const f of paths) {
    try {
      log(await invoke("file_action", { path: f, name: it.id, dest }), "ok");
    } catch (e) {
      log(`${baseName(f)}: ${e}`, "err");
    }
  }
}

/* ------------------------------------------------------------------ modal --- */

function ask(title, fields) {
  return new Promise((resolve) => {
    $("modalTitle").textContent = title;
    const box = $("modalFields");
    box.replaceChildren(
      ...fields.map((f) => {
        const l = document.createElement("label");
        l.className = "field";
        l.textContent = f.label;
        const input = document.createElement("input");
        input.name = f.key;
        input.placeholder = f.placeholder || "";
        input.autocomplete = "off";
        l.append(input);
        return l;
      })
    );
    $("modal").hidden = false;
    box.querySelector("input").focus();
    const finish = (v) => {
      $("modal").hidden = true;
      $("modalForm").onsubmit = null;
      $("modalCancel").onclick = null;
      resolve(v);
    };
    $("modalForm").onsubmit = (e) => {
      e.preventDefault();
      const out = {};
      box.querySelectorAll("input").forEach((i) => (out[i.name] = i.value));
      finish(out);
    };
    $("modalCancel").onclick = () => finish(null);
  });
}

/* ------------------------------------------------------------- modifiers --- */

function onModifiers(next) {
  const pressed = KEYS.filter((k) => next[k] && !mods[k]);
  Object.assign(mods, next);
  if (!pressed.length) return;
  const ring = ringForKey(pressed[0]);
  if (!ring) return;
  if (wheel.open) return setRing(ring);
  if (drag.active) return openWheel(drag.paths, drag.desc, ring, drag.pos, "drag");
}

// Real, system-wide key state from the backend (works during a native file drag).
listen("modifiers", (e) => {
  const p = e.payload;
  onModifiers({ Shift: !!p.shift, Ctrl: !!p.ctrl, Alt: !!p.alt });
});

// Normal key events (wheel already open, dev mode, non-Windows).
window.addEventListener("keydown", (e) => {
  if (!$("modal").hidden) {
    if (e.key === "Escape") $("modalCancel").click();
    return;
  }
  if (!$("panel").hidden) {
    if (e.key === "Escape") $("panelClose").click();
    return;
  }
  const k = e.key === "Shift" ? "Shift" : e.key === "Control" ? "Ctrl" : e.key === "Alt" ? "Alt" : null;
  if (k) {
    if (k === "Alt") e.preventDefault();
    return onModifiers({ ...mods, [k]: true });
  }
  if (!wheel.open) return;
  const n = wheel.items.length;
  if (e.key === "Escape") return closeWheel();
  if (!n) return;
  const move = (d) => {
    e.preventDefault();
    select(wheel.sel < 0 ? (d > 0 ? 0 : n - 1) : (wheel.sel + d + n) % n);
  };
  if (e.key === "ArrowRight" || e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey)) move(1);
  else if (e.key === "ArrowLeft" || e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey)) move(-1);
  else if (e.key === "Enter") {
    e.preventDefault();
    if (wheel.sel >= 0) choose(wheel.sel);
  }
});
window.addEventListener("keyup", (e) => {
  const k = e.key === "Shift" ? "Shift" : e.key === "Control" ? "Ctrl" : e.key === "Alt" ? "Alt" : null;
  if (k) mods[k] = false;
});
window.addEventListener("blur", () => {
  mods.Shift = mods.Ctrl = mods.Alt = false;
});
$("layer").addEventListener("mousedown", (e) => {
  if (e.target === $("layer")) closeWheel();
});

/* ------------------------------------------------------------ drag and drop --- */

const cssPos = (p) => {
  const dpr = window.devicePixelRatio || 1;
  return { x: p.x / dpr, y: p.y / dpr };
};

webview.onDragDropEvent((event) => {
  const p = event.payload;
  if (p.type === "enter") {
    document.body.classList.add("dragging");
    drag.active = true;
    drag.paths = p.paths || [];
    drag.pos = cssPos(p.position);
    drag.desc = drag.paths.length ? describeFiles(drag.paths) : Promise.resolve({ kind: "other", targets: [], tools: [] });
    // A wheel key may already be held when the files enter the window.
    const held = KEYS.find((k) => mods[k] && ringForKey(k));
    if (held && drag.paths.length) openWheel(drag.paths, drag.desc, ringForKey(held), drag.pos, "drag");
  } else if (p.type === "over") {
    drag.pos = cssPos(p.position);
    if (wheel.open && wheel.mode === "drag") {
      const i = hit(drag.pos);
      if (i !== wheel.sel) select(i);
    }
  } else if (p.type === "leave") {
    document.body.classList.remove("dragging");
    drag.active = false;
    if (wheel.open && wheel.mode === "drag") closeWheel();
  } else if (p.type === "drop") {
    document.body.classList.remove("dragging");
    drag.active = false;
    const pos = cssPos(p.position);
    const paths = p.paths && p.paths.length ? p.paths : drag.paths;
    if (!paths.length) return;
    if (wheel.open && wheel.mode === "drag") {
      wheel.mode = "click";
      const i = hit(pos);
      if (i >= 0) return choose(i);
      // Dropped between options: keep the wheel open so the choice is not lost.
      wheel.sel = wheel.items.length ? 0 : -1;
      return renderWheel();
    }
    const held = KEYS.find((k) => mods[k] && ringForKey(k));
    const desc = drag.paths === paths ? drag.desc : describeFiles(paths);
    openWheel(paths, desc || describeFiles(paths), held ? ringForKey(held) : "convert", pos, "click");
  }
});

/* ------------------------------------------------------- settings and tools --- */

function renderHint() {
  const k = (id) => `<kbd>${keyForRing(id)}</kbd>`;
  $("hint").innerHTML = `While dragging, tap ${k("convert")} to convert &middot; ${k("tools")} for tools &middot; ${k("actions")} for actions, then drop on an option.`;
}

function renderKeyRows() {
  const box = $("keyRows");
  box.replaceChildren(
    ...RINGS.map((r) => {
      const row = document.createElement("label");
      row.className = "setting";
      row.append(Object.assign(document.createElement("span"), { textContent: r.label + " wheel" }));
      const sel = document.createElement("select");
      KEYS.forEach((k) => {
        const o = document.createElement("option");
        o.value = k;
        o.textContent = k;
        o.selected = keymap[r.id] === k;
        sel.append(o);
      });
      sel.onchange = () => {
        const other = RINGS.find((x) => x.id !== r.id && keymap[x.id] === sel.value);
        if (other) keymap[other.id] = keymap[r.id]; // swap so each key keeps one job
        keymap[r.id] = sel.value;
        store.set("keymap", keymap);
        renderKeyRows();
        renderHint();
      };
      row.append(sel);
      return row;
    })
  );
}

let toolList = [];

async function loadStatus() {
  try {
    toolList = await invoke("tool_status");
  } catch (e) {
    return console.error(e);
  }
  $("deps").replaceChildren(
    ...toolList.map((t) => {
      const el = document.createElement("button");
      el.className = "pill " + (t.found ? "on" : "off");
      el.textContent = t.label;
      el.title = t.found ? `${t.label} found` : `${t.label} not found: click to install`;
      el.onclick = () => (t.found ? openPanel() : install(t.id));
      return el;
    })
  );
  renderToolRows();
}

function renderToolRows() {
  $("toolRows").replaceChildren(
    ...toolList.map((t) => {
      const row = document.createElement("div");
      row.className = "setting";
      const left = document.createElement("span");
      left.innerHTML = `<b>${t.label}</b><br><small class="muted">${t.used_for}</small>`;
      row.append(left);
      if (t.found) {
        row.append(Object.assign(document.createElement("span"), { className: "pill on", textContent: "Installed" }));
      } else {
        const b = document.createElement("button");
        b.textContent = "Install";
        b.onclick = () => install(t.id, b);
        row.append(b);
      }
      return row;
    })
  );
  $("installAll").hidden = toolList.every((t) => t.found);
}

async function install(id, btn) {
  const t = toolList.find((x) => x.id === id);
  const li = log(`Installing ${t ? t.label : id}… this can take a minute`, "run");
  if (btn) btn.disabled = true;
  try {
    li.className = "ok";
    li.firstChild.textContent = await invoke("install_tool", { id });
  } catch (e) {
    fail(li, String(e));
  }
  await loadStatus();
}

async function installAll() {
  for (const t of toolList.filter((x) => !x.found)) await install(t.id);
}

function openPanel() {
  renderKeyRows();
  renderToolRows();
  $("panel").hidden = false;
}

$("settingsBtn").onclick = openPanel;
$("panelClose").onclick = () => ($("panel").hidden = true);
$("installAll").onclick = installAll;
$("pinBtn").onclick = () => setFloat(!floating);

/* ------------------------------------------------------------------- start --- */

renderHint();
loadStatus();
if (floating) setFloat(true);
else $("pinBtn").setAttribute("aria-pressed", "false");
invoke("stats")
  .then((s) => ($("stats").textContent = `${s.conversions} conversion options · ${s.tools} file tools · everything stays on this PC`))
  .catch(() => {});
