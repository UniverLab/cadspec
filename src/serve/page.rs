//! The built-in viewer page: HTML shell, favicon, and title injection.

use super::inspect::html_escape;

pub(super) fn index_html(project_name: &str) -> String {
    INDEX_HTML.replace("{{PROJECT_NAME}}", &html_escape(project_name))
}

pub(super) const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="6" fill="#0d0e11"/><text x="16" y="22" font-family="monospace" font-size="16" font-weight="700" fill="#6ec6e6" text-anchor="middle">cs</text></svg>"##;

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{{PROJECT_NAME}} — cadspec live</title>
<link rel="icon" href="/favicon.svg">
<style>
  @import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&display=swap');
  /* Dark only — this is the lab, not the plan-white. */
  :root {
    --bg: #0d0e11; --panel: #14161a; --panel-2: #181b20;
    --ink: #d6dadf; --ink-strong: #ffffff; --ink-dim: #aab0b8;
    --ink-mute: #7c828b; --ink-faint: #565b63;
    --line: #23262c; --line-2: #2e323a;
    --accent: #6ec6e6; --accent-soft: rgba(110,198,230,0.16);
    --ok: #5dd39e; --err: #e0746e; --err-soft: #2a1212; --err-ink: #ff9f9a;
    --code-bg: #0a0b0d; --code-ink: #cdd6df;
    --t-num: #d9a35f; --t-hdr: #c79be0;
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { background: var(--bg); color: var(--ink); font-family: 'IBM Plex Mono', ui-monospace, 'Cascadia Code', monospace; height: 100vh; display: flex; flex-direction: column; overflow: hidden; }
  header { display: flex; align-items: center; gap: 12px; padding: 9px 14px; background: var(--panel-2); border-bottom: 1px solid var(--line-2); user-select: none; }
  #dot { width: 10px; height: 10px; border-radius: 50%; background: var(--ok); flex: none; transition: background .2s; }
  #dot.err { background: var(--err); }
  #title { font-weight: 600; color: var(--ink-strong); white-space: nowrap; }
  .tag { font-size: 11px; color: var(--ink-mute); border: 1px solid var(--line-2); border-radius: 4px; padding: 2px 7px; white-space: nowrap; }
  button { background: var(--panel); color: var(--ink-dim); border: 1px solid var(--line-2); border-radius: 4px; padding: 3px 10px; font: inherit; font-size: 11px; cursor: pointer; }
  button:hover { color: var(--ink-strong); border-color: var(--ink-faint); }
  button.active { color: var(--accent); border-color: var(--accent); }
  #hint { margin-left: auto; font-size: 11px; color: var(--ink-faint); }
  main { flex: 1; display: flex; overflow: hidden; }
  /* built-in .cf editor */
  #editor { width: 340px; min-width: 200px; max-width: 720px; flex: none; background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; }
  #editor.hidden { display: none; }
  .ed-head { display: flex; gap: 6px; padding: 7px 8px; border-bottom: 1px solid var(--line); }
  #ed-file { flex: 1; min-width: 0; background: var(--panel-2); color: var(--ink); border: 1px solid var(--line-2); border-radius: 4px; font: inherit; font-size: 11px; padding: 3px 6px; }
  /* highlighted-overlay editor: a coloured <pre> behind a transparent textarea */
  #ed-wrap { flex: 1; position: relative; overflow: hidden; background: var(--code-bg); }
  #ed-hl, #ed-text { position: absolute; inset: 0; margin: 0; border: 0; padding: 10px 12px; font: inherit; font-size: 12px; line-height: 1.55; tab-size: 2; white-space: pre; overflow: auto; }
  #ed-hl { color: var(--code-ink); pointer-events: none; z-index: 0; }
  #ed-hl code { font: inherit; }
  #ed-text { resize: none; background: transparent; color: transparent; caret-color: var(--ink-strong); outline: none; z-index: 1; }
  #ed-text::selection { background: var(--accent-soft); }
  .t-key { color: var(--accent); }
  .t-str { color: var(--ok); }
  .t-num { color: var(--t-num); }
  .t-hdr { color: var(--t-hdr); font-weight: 500; }
  .t-com { color: var(--ink-faint); font-style: italic; }
  .t-bool { color: var(--err); }
  #ed-status { padding: 5px 10px; font-size: 10px; color: var(--ink-faint); border-top: 1px solid var(--line); white-space: nowrap; overflow: hidden; }
  #ed-status.ok { color: var(--ok); }
  #ed-status.err { color: var(--err); }
  #ed-status.dirty { color: var(--accent); }
  #editor-resizer { width: 6px; flex: none; cursor: col-resize; background: transparent; transition: background .15s; }
  #editor-resizer.hidden { display: none; }
  #editor-resizer:hover, #editor-resizer.dragging { background: var(--accent); }
  /* left sidebar: two stacked panes (Layers over Planos), IntelliJ-style */
  #sidebar { width: 200px; min-width: 130px; max-width: 560px; flex: none; background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; user-select: none; }
  #layers-pane { flex: 1 1 auto; overflow-y: auto; padding: 8px; min-height: 48px; }
  #planos-pane { flex: none; height: 40%; overflow-y: auto; padding: 8px; min-height: 48px; }
  /* horizontal divider between the two panes */
  #pane-divider { height: 6px; flex: none; cursor: row-resize; background: var(--panel-2); border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); transition: background .15s; }
  #pane-divider:hover, #pane-divider.dragging { background: var(--accent); }
  /* drag handle to resize the whole sidebar width */
  #layers-resizer { width: 6px; flex: none; cursor: col-resize; background: transparent; transition: background .15s; }
  #layers-resizer:hover, #layers-resizer.dragging { background: var(--accent); }
  #sidebar h3 { font-size: 10px; color: var(--ink-faint); text-transform: uppercase; letter-spacing: 1px; margin: 2px 0 8px 4px; }
  .layer-row { display: flex; align-items: center; gap: 7px; padding: 5px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .layer-row:hover, .plano-row:hover { background: var(--panel-2); }
  .layer-dot { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .layer-row .st { margin-left: auto; font-size: 10px; color: var(--ink-faint); }
  .layer-row.ghost { color: var(--ink-mute); }
  .layer-row.off { color: var(--ink-faint); }
  .plano-row { display: flex; align-items: baseline; gap: 7px; padding: 5px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .plano-row .pv { margin-left: auto; font-size: 9px; color: var(--ink-faint); text-transform: uppercase; }
  .plano-row.active { background: var(--accent-soft); color: var(--accent); }
  #planos-pane .empty { font-size: 11px; color: var(--ink-faint); padding: 4px 6px; line-height: 1.5; }
  /* viewport */
  #viewport { flex: 1; overflow: hidden; position: relative; cursor: grab; perspective: 2200px; background: var(--bg); }
  #viewport.panning { cursor: grabbing; }
  #viewport.is3d { cursor: default; }
  /* interactive WebGL (glTF) layer, shown in 3D mode */
  #gl { position: absolute; inset: 0; width: 100%; height: 100%; display: none; }
  #gl.show { display: block; }
  #canvas { position: absolute; transform-origin: 0 0; will-change: transform; transform-style: preserve-3d; }
  #canvas svg { display: block; }
  .plane { position: absolute; left: 0; top: 0; }
  /* entity interaction */
  #canvas [data-id] { cursor: pointer; }
  #canvas [data-id]:hover { filter: brightness(1.8); }
  #canvas .sel { filter: drop-shadow(0 0 5px var(--accent)) brightness(1.6); }
  /* inspector */
  #inspector { width: 300px; flex: none; background: var(--panel); border-left: 1px solid var(--line); padding: 12px; overflow-y: auto; display: none; }
  #inspector.show { display: block; }
  #inspector h2 { font-size: 13px; color: var(--accent); word-break: break-all; }
  #inspector .meta { font-size: 11px; color: var(--ink-mute); margin: 6px 0 10px; line-height: 1.6; }
  #inspector pre { background: var(--code-bg); border: 1px solid var(--line-2); border-radius: 5px; padding: 9px; font-size: 11px; line-height: 1.45; white-space: pre-wrap; word-break: break-all; color: var(--code-ink); }
  #inspector .btns { display: flex; gap: 6px; margin-top: 10px; flex-wrap: wrap; }
  #inspector .note { font-size: 10px; color: var(--ink-faint); margin-top: 8px; }
  #error { display: none; position: absolute; left: 16px; right: 16px; bottom: 16px; background: var(--err-soft); border: 1px solid var(--err); border-radius: 6px; padding: 12px 16px; color: var(--err-ink); font-size: 13px; white-space: pre-wrap; max-height: 40%; overflow: auto; z-index: 10; }
  #error.show { display: block; }
  #toast { position: fixed; bottom: 44px; left: 50%; transform: translateX(-50%); background: var(--panel-2); border: 1px solid var(--accent); color: var(--accent); font-size: 11px; padding: 5px 14px; border-radius: 4px; opacity: 0; transition: opacity .2s; pointer-events: none; z-index: 20; }
  #toast.show { opacity: 1; }
  footer { padding: 5px 14px; background: var(--panel); border-top: 1px solid var(--line); font-size: 11px; color: var(--ink-faint); user-select: none; }
</style>
</head>
<body>
<header>
  <span id="dot"></span>
  <span id="title">{{PROJECT_NAME}}</span>
  <span class="tag">cadspec live</span>
  <span class="tag" id="version">v0</span>
  <button id="btn3d" title="extruded 3D view (key: 3)">3D</button>
  <button id="btnfit" title="fit to view (key: F)">fit</button>
  <button id="btneditor" class="active" title="toggle editor (key: E)">editor</button>
  <span id="hint">edit .cf files — preview updates automatically</span>
</header>
<main>
  <aside id="editor">
    <div class="ed-head">
      <select id="ed-file" title="project .cf files"></select>
      <button id="ed-save" title="save (Ctrl+S)">save</button>
    </div>
    <div id="ed-wrap">
      <pre id="ed-hl" aria-hidden="true"><code></code></pre>
      <textarea id="ed-text" spellcheck="false" autocapitalize="off" autocomplete="off" placeholder="select a .cf file…"></textarea>
    </div>
    <div id="ed-status">ready</div>
  </aside>
  <div id="editor-resizer" title="drag to resize the editor"></div>
  <aside id="sidebar">
    <div id="layers-pane"><h3>Layers</h3><div id="layerlist"></div></div>
    <div id="pane-divider" title="drag to resize panes"></div>
    <div id="planos-pane"><h3>Planos</h3><div id="planoslist"></div></div>
  </aside>
  <div id="layers-resizer" title="drag to resize · double-click to reset"></div>
  <div id="viewport">
    <div id="canvas"></div>
    <canvas id="gl"></canvas>
    <pre id="error"></pre>
  </div>
  <aside id="inspector">
    <h2 id="ins-id"></h2>
    <div class="meta" id="ins-meta"></div>
    <pre id="ins-block"></pre>
    <div class="btns">
      <button id="copy-id">copy id</button>
      <button id="copy-toml">copy TOML</button>
      <button id="copy-agent">copy for agent</button>
      <button id="close-ins">close</button>
    </div>
    <div class="note" id="ins-note"></div>
  </aside>
</main>
<div id="toast"></div>
<footer>click: inspect entity · scroll: zoom · drag: pan · double-click: fit · 1-9: cycle layer (on/ghost/off) · 3: 3D · Esc: deselect</footer>
<script>
const canvas = document.getElementById('canvas');
const viewport = document.getElementById('viewport');
const dot = document.getElementById('dot');
const errBox = document.getElementById('error');
const versionTag = document.getElementById('version');
const layerList = document.getElementById('layerlist');
const planosList = document.getElementById('planoslist');
const inspector = document.getElementById('inspector');
const toast = document.getElementById('toast');

let scale = 1, tx = 0, ty = 0;
let fitted = false;
let mode3d = false;
let svgText = '';
let svg3dText = '';
let layersInfo = [];                 // [{name, color}]
let planosInfo = [];                 // [{name, view, title}]
let currentPlano = null;             // active plano name, or null for the model
let planoSvg = '';
const layerState = {};               // name → 'on' | 'ghost' | 'off'
let selectedId = null;

// ── transform / view ────────────────────────────────────────────────
function applyTransform() {
  // The 3D view is a real axonometric projection baked into the SVG, so the
  // canvas only ever needs pan + zoom (no CSS tilt).
  canvas.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
}
function svgSize() {
  const svg = canvas.querySelector('svg');
  if (!svg) return null;
  return { w: parseFloat(svg.getAttribute('width')), h: parseFloat(svg.getAttribute('height')) };
}
function fitToView() {
  if (mode3d) { if (window.gl3d) window.gl3d.frame(); return; }
  const s = svgSize();
  if (!s) return;
  const vw = viewport.clientWidth, vh = viewport.clientHeight;
  scale = Math.min(vw / s.w, vh / s.h) * 0.96;
  tx = (vw - s.w * scale) / 2;
  ty = (vh - s.h * scale) / 2;
  applyTransform();
  fitted = true;
}

// ── rendering ───────────────────────────────────────────────────────
function renderCanvas() {
  if (mode3d) return;                 // 3D is the WebGL (glTF) layer, not the SVG
  const content = currentPlano ? planoSvg : svgText;
  if (!content) return;
  canvas.innerHTML = content;
  applyLayerStates();
  applySelection();
}

function applyLayerStates() {
  // 2D tags layers on <g>; the 3D view tags each projected face — match both.
  canvas.querySelectorAll('[data-layer]').forEach(g => {
    const st = layerState[g.dataset.layer] || 'on';
    g.style.opacity = st === 'on' ? '' : st === 'ghost' ? '0.16' : '0';
    g.style.pointerEvents = st === 'on' ? '' : 'none';
  });
}

function renderLayerPanel() {
  layerList.innerHTML = '';
  layersInfo.forEach((l, i) => {
    const st = layerState[l.name] || 'on';
    const row = document.createElement('div');
    row.className = 'layer-row' + (st !== 'on' ? ' ' + st : '');
    row.innerHTML = `<span class="layer-dot" style="background:${l.color}"></span>` +
                    `<span>${l.name}</span><span class="st">${i + 1} · ${st}</span>`;
    row.onclick = () => cycleLayer(l.name);
    layerList.appendChild(row);
  });
}
function cycleLayer(name) {
  const next = { on: 'ghost', ghost: 'off', off: 'on' };
  layerState[name] = next[layerState[name] || 'on'];
  renderLayerPanel();
  applyLayerStates();
}

// ── planos panel ────────────────────────────────────────────────────
function renderPlanosPanel() {
  planosList.innerHTML = '';
  if (!planosInfo.length) {
    planosList.innerHTML = '<div class="empty">no planos — add [[plano]] to project.toml</div>';
    return;
  }
  planosInfo.forEach(p => {
    const row = document.createElement('div');
    row.className = 'plano-row' + (currentPlano === p.name ? ' active' : '');
    row.innerHTML = `<span>${p.title || p.name}</span><span class="pv">${p.view}</span>`;
    row.onclick = () => openPlano(p.name);
    planosList.appendChild(row);
  });
}
async function fetchPlano(name) {
  return (await fetch('/plano.svg?name=' + encodeURIComponent(name) + '&t=' + Date.now())).text();
}
async function openPlano(name) {
  if (currentPlano === name) {           // toggle off → back to the model
    currentPlano = null; planoSvg = '';
    renderPlanosPanel(); renderCanvas(); fitToView();
    return;
  }
  currentPlano = name;
  renderPlanosPanel();
  planoSvg = await fetchPlano(name);
  renderCanvas();
  fitToView();
}

// ── selection / inspector ───────────────────────────────────────────
function applySelection() {
  canvas.querySelectorAll('.sel').forEach(n => n.classList.remove('sel'));
  if (!selectedId) return;
  canvas.querySelectorAll(`[data-id="${CSS.escape(selectedId)}"]`)
    .forEach(n => n.classList.add('sel'));
}
async function select(id) {
  selectedId = id;
  applySelection();
  const info = await (await fetch('/entity?id=' + encodeURIComponent(id))).json();
  document.getElementById('ins-id').textContent = id;
  if (info.error) {
    document.getElementById('ins-meta').textContent = 'no source block found (entity has no id?)';
    document.getElementById('ins-block').textContent = '';
    document.getElementById('ins-note').textContent = '';
  } else {
    document.getElementById('ins-meta').innerHTML =
      `layer: <b>${info.layer}</b> · file: <b>${info.file}</b>`;
    document.getElementById('ins-block').textContent = info.block;
    document.getElementById('ins-note').textContent = info.generated
      ? `⚠ generated copy — source is "${info.base_id}" (edit its [[array]]/[[mirror]])` : '';
    inspector.dataset.file = info.file;
    inspector.dataset.block = info.block;
  }
  inspector.classList.add('show');
}
function deselect() {
  selectedId = null;
  applySelection();
  inspector.classList.remove('show');
}
function copyText(text, msg) {
  navigator.clipboard.writeText(text).then(() => {
    toast.textContent = msg;
    toast.classList.add('show');
    setTimeout(() => toast.classList.remove('show'), 1200);
  });
}
document.getElementById('copy-id').onclick = () => copyText(selectedId, 'id copied');
document.getElementById('copy-toml').onclick = () =>
  copyText(`# ${inspector.dataset.file}\n${inspector.dataset.block}`, 'TOML copied');
document.getElementById('copy-agent').onclick = () =>
  copyText(`In ${inspector.dataset.file}, modify the entity "${selectedId}". Current definition:\n\n` +
           '```toml\n' + inspector.dataset.block + '\n```', 'agent prompt copied');
document.getElementById('close-ins').onclick = deselect;

// ── data refresh ────────────────────────────────────────────────────
async function refresh() {
  const [stateRes, svgRes, svg3dRes] = await Promise.all([
    fetch('/state'), fetch('/preview.svg?t=' + Date.now()), fetch('/preview3d.svg?t=' + Date.now())
  ]);
  const state = await stateRes.json();
  versionTag.textContent = 'v' + state.version;
  layersInfo = state.layers || [];
  planosInfo = state.planos || [];
  renderLayerPanel();
  renderPlanosPanel();
  if (state.error) {
    dot.classList.add('err');
    errBox.textContent = state.error;
    errBox.classList.add('show');
  } else {
    dot.classList.remove('err');
    errBox.classList.remove('show');
    svgText = await svgRes.text();
    svg3dText = await svg3dRes.text();
    // The active plano may reference changed geometry — re-render it too.
    if (currentPlano) {
      if (planosInfo.some(p => p.name === currentPlano)) planoSvg = await fetchPlano(currentPlano);
      else { currentPlano = null; planoSvg = ''; }  // plano was removed
    }
    renderCanvas();
    if (!fitted) fitToView();
    if (mode3d && window.gl3d) window.gl3d.reload();
  }
  if (window.__reloadEditorIfClean) window.__reloadEditorIfClean();
}

// ── input ───────────────────────────────────────────────────────────
viewport.addEventListener('wheel', e => {
  if (mode3d) return;                 // OrbitControls handles zoom in 3D
  e.preventDefault();
  const factor = Math.exp(-e.deltaY * 0.0012);
  const next = Math.min(Math.max(scale * factor, 0.05), 50);
  const r = viewport.getBoundingClientRect();
  const mx = e.clientX - r.left, my = e.clientY - r.top;
  tx = mx - (mx - tx) * (next / scale);
  ty = my - (my - ty) * (next / scale);
  scale = next;
  applyTransform();
}, { passive: false });

let panning = false, moved = 0, px = 0, py = 0;
viewport.addEventListener('mousedown', e => {
  if (mode3d) return;                 // OrbitControls handles rotate/pan in 3D
  panning = true; moved = 0; px = e.clientX; py = e.clientY;
  viewport.classList.add('panning');
});
window.addEventListener('mousemove', e => {
  if (!panning) return;
  moved += Math.abs(e.clientX - px) + Math.abs(e.clientY - py);
  tx += e.clientX - px; ty += e.clientY - py;
  px = e.clientX; py = e.clientY;
  applyTransform();
});
window.addEventListener('mouseup', () => {
  panning = false;
  viewport.classList.remove('panning');
});
viewport.addEventListener('click', e => {
  if (mode3d) return;                          // no entity-picking in 3D
  if (moved > 5) return;                       // it was a pan, not a click
  const el = e.target.closest('[data-id]');
  if (el) select(el.getAttribute('data-id'));
  else deselect();
});
viewport.addEventListener('dblclick', fitToView);

function toggle3d() {
  mode3d = !mode3d;
  document.getElementById('btn3d').classList.toggle('active', mode3d);
  viewport.classList.toggle('is3d', mode3d);
  if (mode3d) {
    canvas.style.display = 'none';
    if (window.gl3d) window.gl3d.mount();
  } else {
    canvas.style.display = '';
    if (window.gl3d) window.gl3d.unmount();
    renderCanvas();
    fitToView();
  }
}
document.getElementById('btn3d').onclick = toggle3d;
document.getElementById('btnfit').onclick = fitToView;

window.addEventListener('keydown', e => {
  if (e.target.tagName === 'INPUT') return;
  if (e.key === 'Escape') deselect();
  else if (e.key === 'f' || e.key === 'F') fitToView();
  else if (e.key === '3') toggle3d();
  else if (e.key >= '1' && e.key <= '9') {
    const l = layersInfo[+e.key - 1];
    if (l) cycleLayer(l.name);
  }
});

// ── resizable sidebar (width) ───────────────────────────────────────
(function () {
  const sidebar = document.getElementById('sidebar');
  const rz = document.getElementById('layers-resizer');
  const KEY = 'cadspec.layersWidth', MIN = 130, MAX = 560, DEF = 200;
  const saved = parseInt(localStorage.getItem(KEY) || '', 10);
  if (saved >= MIN && saved <= MAX) sidebar.style.width = saved + 'px';
  let dragging = false;
  rz.addEventListener('mousedown', e => {
    dragging = true; rz.classList.add('dragging');
    document.body.style.cursor = 'col-resize'; e.preventDefault();
  });
  window.addEventListener('mousemove', e => {
    if (!dragging) return;
    const w = Math.min(MAX, Math.max(MIN, e.clientX - sidebar.getBoundingClientRect().left));
    sidebar.style.width = w + 'px';
  });
  window.addEventListener('mouseup', () => {
    if (!dragging) return;
    dragging = false; rz.classList.remove('dragging'); document.body.style.cursor = '';
    localStorage.setItem(KEY, parseInt(sidebar.style.width, 10));
  });
  rz.addEventListener('dblclick', () => {
    sidebar.style.width = DEF + 'px'; localStorage.removeItem(KEY);
  });
})();

// ── stacked panes: drag the Layers/Planos divider (height) ──────────
(function () {
  const sidebar = document.getElementById('sidebar');
  const pane = document.getElementById('planos-pane');
  const div = document.getElementById('pane-divider');
  const KEY = 'cadspec.planosHeight';
  const saved = parseInt(localStorage.getItem(KEY) || '', 10);
  if (saved >= 48) pane.style.height = saved + 'px';
  let dragging = false;
  div.addEventListener('mousedown', e => {
    dragging = true; div.classList.add('dragging');
    document.body.style.cursor = 'row-resize'; e.preventDefault();
  });
  window.addEventListener('mousemove', e => {
    if (!dragging) return;
    const r = sidebar.getBoundingClientRect();
    const h = Math.min(r.height - 60, Math.max(48, r.bottom - e.clientY));
    pane.style.height = h + 'px';
  });
  window.addEventListener('mouseup', () => {
    if (!dragging) return;
    dragging = false; div.classList.remove('dragging'); document.body.style.cursor = '';
    localStorage.setItem(KEY, parseInt(pane.style.height, 10));
  });
})();

const events = new EventSource('/events');
events.onmessage = refresh;
events.onerror = () => dot.classList.add('err');

refresh();
</script>
<script>
  // ── Built-in .cf editor: syntax highlight + debounced auto-save ─────────────
  (function () {
    var editor = document.getElementById('editor');
    var resizer = document.getElementById('editor-resizer');
    var sel = document.getElementById('ed-file');
    var text = document.getElementById('ed-text');
    var hl = document.querySelector('#ed-hl code');
    var saveBtn = document.getElementById('ed-save');
    var status = document.getElementById('ed-status');
    var toggle = document.getElementById('btneditor');
    var current = null; // the file currently loaded in the textarea
    var timer = null;
    var SAVE_DELAY = 600;
    var dirty = false;     // unsaved edits pending (or in-flight)
    var lastInput = 0;     // Date.now() of the last keystroke

    function setStatus(msg, cls) { status.textContent = msg; status.className = cls || ''; }
    function escHtml(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }

    // Lightweight .cf (TOML-ish) tokenizer → coloured spans.
    function highlight(code) {
      return code.split('\n').map(function (line) {
        var out = '', rest = escHtml(line);
        var km = rest.match(/^(\s*)([A-Za-z0-9_.\-]+)(\s*=)/);
        if (km) { out += km[1] + '<span class="t-key">' + km[2] + '</span>' + km[3]; rest = rest.slice(km[0].length); }
        out += rest.replace(/(#.*$)|("(?:[^"\\]|\\.)*")|(\[\[?[^\]]*\]\]?)|(-?\b\d+\.?\d*\b)|(\btrue\b|\bfalse\b)/g,
          function (m, c, s, h, n, b) {
            if (c) return '<span class="t-com">' + c + '</span>';
            if (s) return '<span class="t-str">' + s + '</span>';
            if (h) return '<span class="t-hdr">' + h + '</span>';
            if (n) return '<span class="t-num">' + n + '</span>';
            if (b) return '<span class="t-bool">' + b + '</span>';
            return m;
          });
        return out;
      }).join('\n');
    }
    function paint() { hl.innerHTML = highlight(text.value) + '\n'; }
    function syncScroll() { var p = hl.parentNode; p.scrollTop = text.scrollTop; p.scrollLeft = text.scrollLeft; }

    function save() {
      var name = current; // captured: stays correct even if the file switches
      if (!name) return;
      clearTimeout(timer); timer = null;
      setStatus('saving…');
      fetch('/save?name=' + encodeURIComponent(name), { method: 'POST', body: text.value })
        .then(function (r) { return r.json(); })
        .then(function (j) {
          dirty = false;
          setStatus(j.ok ? 'saved · ' + name : 'saved · build error (see viewer)', j.ok ? 'ok' : 'err');
        })
        .catch(function () { setStatus('save failed', 'err'); });
    }
    function scheduleSave() {
      clearTimeout(timer);
      setStatus('● ' + current, 'dirty');
      timer = setTimeout(save, SAVE_DELAY);
    }
    function loadFile(name) {
      clearTimeout(timer); timer = null;
      fetch('/file?name=' + encodeURIComponent(name))
        .then(function (r) { return r.text(); })
        .then(function (t) { current = name; text.value = t; paint(); syncScroll(); setStatus(name); dirty = false; })
        .catch(function () { setStatus('cannot load ' + name, 'err'); });
    }
    function loadFiles() {
      fetch('/files').then(function (r) { return r.json(); }).then(function (j) {
        sel.innerHTML = '';
        (j.files || []).forEach(function (f) {
          var o = document.createElement('option');
          o.value = f; o.textContent = f; sel.appendChild(o);
        });
        if (sel.options.length) { loadFile(sel.value); }
        else { current = null; text.value = ''; paint(); setStatus('no .cf files'); }
      }).catch(function () { setStatus('cannot list files', 'err'); });
    }

    sel.addEventListener('change', function () {
      if (timer) save();          // flush the pending edit to the old file first
      loadFile(sel.value);
    });
    text.addEventListener('input', function () {
      dirty = true; lastInput = Date.now();
      paint(); scheduleSave();
    });
    text.addEventListener('scroll', syncScroll);
    saveBtn.addEventListener('click', save);
    // Keep editor keystrokes out of the viewer's shortcuts (3 / f / 1-9 / esc).
    text.addEventListener('keydown', function (e) {
      e.stopPropagation();
      if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) { e.preventDefault(); save(); }
    });
    sel.addEventListener('keydown', function (e) { e.stopPropagation(); });

    toggle.addEventListener('click', function () {
      var hidden = editor.classList.toggle('hidden');
      resizer.classList.toggle('hidden', hidden);
      toggle.classList.toggle('active', !hidden);
    });
    window.addEventListener('keydown', function (e) {
      var tag = (e.target && e.target.tagName) || '';
      if ((e.key === 'e' || e.key === 'E') && !/INPUT|TEXTAREA|SELECT/.test(tag)) toggle.click();
    });

    // Drag to resize the editor pane.
    var drag = false;
    resizer.addEventListener('mousedown', function (e) { drag = true; resizer.classList.add('dragging'); e.preventDefault(); });
    window.addEventListener('mousemove', function (e) {
      if (!drag) return;
      editor.style.width = Math.max(200, Math.min(720, e.clientX)) + 'px';
    });
    window.addEventListener('mouseup', function () { drag = false; resizer.classList.remove('dragging'); });

    // Called on every SSE tick so external edits (another editor, git checkout)
    // get picked up — but never while the user has unsaved or in-progress edits.
    window.__reloadEditorIfClean = function () {
      if (!current) return;
      var editingNow = document.activeElement === text && (Date.now() - lastInput) < 2000;
      if (dirty || editingNow) return;   // don't clobber in-progress edits
      loadFile(current);
    };

    loadFiles();
  })();
</script>
<script type="importmap">
{
  "imports": {
    "three": "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
    "three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/"
  }
}
</script>
<script type="module">
  // Interactive 3D: load the scene as glTF and orbit it. The flat axonometric
  // SVG is no longer used for 3D — this is the real model.
  import * as THREE from 'three';
  import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
  import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

  var glCanvas = document.getElementById('gl');
  var renderer, scene, camera, controls, model, raf = null, inited = false;
  var loader = new GLTFLoader();

  function bgColor() {
    var c = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim();
    return new THREE.Color(c || '#0d0e11');
  }
  function resize() {
    if (!renderer) return;
    var w = glCanvas.clientWidth, h = glCanvas.clientHeight;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h; camera.updateProjectionMatrix();
  }
  function init() {
    if (inited) return; inited = true;
    renderer = new THREE.WebGLRenderer({ canvas: glCanvas, antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    scene = new THREE.Scene();
    camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1e6);
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true; controls.dampingFactor = 0.08;
    scene.add(new THREE.AmbientLight(0xffffff, 0.6));
    var d1 = new THREE.DirectionalLight(0xffffff, 0.85); d1.position.set(1, 2, 1.5); scene.add(d1);
    var d2 = new THREE.DirectionalLight(0xffffff, 0.3); d2.position.set(-1.2, -0.4, -1); scene.add(d2);
    window.addEventListener('resize', resize);
  }
  function frame() {
    if (!model) return;
    var box = new THREE.Box3().setFromObject(model);
    var size = box.getSize(new THREE.Vector3());
    var center = box.getCenter(new THREE.Vector3());
    var r = Math.max(size.x, size.y, size.z, 1) * 0.5;
    controls.target.copy(center);
    var dist = r / Math.tan((camera.fov * Math.PI / 180) / 2) * 1.7;
    camera.position.set(center.x + dist * 0.8, center.y + dist * 0.7, center.z + dist * 0.9);
    camera.near = Math.max(r / 200, 0.001); camera.far = r * 200; camera.updateProjectionMatrix();
    controls.update();
  }
  function load() {
    loader.load('/scene.gltf?t=' + Date.now(), function (g) {
      if (model) scene.remove(model);
      model = g.scene; scene.add(model); frame();
    }, undefined, function () {});
  }
  function loop() {
    raf = requestAnimationFrame(loop);
    controls.update();
    renderer.setClearColor(bgColor(), 1);
    renderer.render(scene, camera);
  }
  window.gl3d = {
    mount: function () { init(); glCanvas.classList.add('show'); resize(); load(); if (!raf) loop(); },
    unmount: function () { glCanvas.classList.remove('show'); if (raf) { cancelAnimationFrame(raf); raf = null; } },
    reload: function () { if (glCanvas.classList.contains('show')) load(); },
    frame: function () { frame(); }
  };
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_injects_project_name() {
        let html = index_html("Casa <Lote 12>");
        assert!(html.contains("Casa &lt;Lote 12&gt;"));
        assert!(!html.contains("{{PROJECT_NAME}}"));
    }

    #[test]
    fn index_html_has_favicon_and_branding() {
        let html = index_html("proj");
        assert!(html.contains("favicon.svg"));
        assert!(html.contains("cadspec"));
    }

    #[test]
    fn index_html_is_dark_only_no_theme_toggle() {
        let html = index_html("proj");
        assert!(!html.contains("data-theme"));
        assert!(!html.contains("btntheme"));
        assert!(!html.contains("prefers-color-scheme"));
    }

    #[test]
    fn index_html_exposes_editor_reload_hook() {
        let html = index_html("proj");
        assert!(html.contains("__reloadEditorIfClean"));
    }
}
