import { GtGraphic } from "./gt-graphic.js";

const $ = (id) => document.getElementById(id);
const status = (text) => { $("status").textContent = text; };

let wasm = null;
let documentIr = emptyDoc();
let selectedName = "";
let selectedAnim = null;
let previewOn = true;
let playing = false;
let playAnchor = 0;
let playStart = 0;
let currentTime = 0;
let hold = 2;
let pps = 60;
let drag = null;
let assets = {};
let history = [];

void GtGraphic;

function emptyDoc() {
  return {
    width: 1920,
    height: 1080,
    layers: [],
    storyboards: [],
    unknown_children: [],
    extra_attrs: {},
    warnings: [],
    asset_names: [],
  };
}

async function loadWasm() {
  try {
    wasm = await import("./pkg/gt_wasm.js");
    if (wasm.default) await wasm.default();
    status("Wasm ready");
  } catch {
    status("Wasm not built. Run wasm-pack in crates/gt-wasm --out-dir ../../web/editor/pkg");
  }
}

function objects() {
  const out = [];
  for (const layer of documentIr.layers || []) walk(layer, out);
  return out;
}

function walk(layer, out) {
  for (const child of layer.objects || []) {
    if (child.node === "object" || child.kind) out.push(child);
    if (child.node === "layer" || child.objects) walk(child, out);
  }
}

function flattenObject(child) {
  return child.node === "object" ? child : child;
}

function storyboardViews() {
  const boards = documentIr.storyboards || [];
  const views = boards.map((board, index) => ({
    label: board.storyboard_type || "TransitionIn",
    segments: [{ storyboard_index: index, offset: 0 }],
  }));
  const inn = boards.findIndex((board) => !board.storyboard_type || board.storyboard_type === "TransitionIn");
  const out = boards.findIndex((board) => board.storyboard_type === "TransitionOut");
  if (inn >= 0 && out >= 0) {
    views.push({
      label: "TransitionIn + TransitionOut",
      segments: [
        { storyboard_index: inn, offset: 0 },
        { storyboard_index: out, offset: durationOf(boards[inn]) + Number($("hold").value || hold) },
      ],
    });
  }
  const dcIn = boards
    .map((board, index) => ({ board, index }))
    .filter((item) => item.board.storyboard_type === "DataChangeIn");
  const dcOut = boards
    .map((board, index) => ({ board, index }))
    .filter((item) => item.board.storyboard_type === "DataChangeOut");
  if (dcIn.length && dcOut.length) {
    const inDur = Math.max(...dcIn.map((item) => durationOf(item.board)));
    views.push({
      label: "DataChange In+Out",
      segments: [
        ...dcIn.map((item) => ({ storyboard_index: item.index, offset: 0 })),
        ...dcOut.map((item) => ({ storyboard_index: item.index, offset: inDur })),
      ],
    });
  }
  return views;
}

function pushHistory() {
  history.push(JSON.stringify(documentIr));
  if (history.length > 40) history.shift();
}

function undo() {
  const prev = history.pop();
  if (!prev) return;
  documentIr = JSON.parse(prev);
  refreshStoryboardSelect();
  renderStage();
}

function durationOf(board) {
  return (board.animations || []).reduce((max, anim) => {
    const end = Number(anim.delay || 0) + Number(anim.duration || 1);
    return Math.max(max, end);
  }, 0);
}

function currentView() {
  return storyboardViews()[Number($("storyboard").selectedIndex) || 0] || { segments: [], label: "" };
}

function viewDuration() {
  const view = currentView();
  return view.segments.reduce((max, segment) => {
    const board = documentIr.storyboards[segment.storyboard_index];
    return Math.max(max, segment.offset + durationOf(board || { animations: [] }));
  }, 0);
}

function refreshStoryboardSelect() {
  const select = $("storyboard");
  const current = select.value;
  select.innerHTML = "";
  for (const view of storyboardViews()) {
    const option = document.createElement("option");
    option.textContent = view.label;
    select.append(option);
  }
  if (current) select.value = current;
}

async function renderStage() {
  if (!wasm?.to_html) return;
  const html = wasm.to_html(JSON.stringify(documentIr), "TransitionIn");
  $("graphic").setHtml(html);
  $("ir").value = JSON.stringify(documentIr, null, 2);
  await seek(currentTime);
}

async function seek(time) {
  currentTime = Math.max(0, time);
  $("time-label").textContent = `${currentTime.toFixed(2)} / ${viewDuration().toFixed(2)}s`;
  drawTrack();
  if (!previewOn || !wasm?.evaluate_view) {
    $("graphic").clearOverrides();
    return;
  }
  const frame = JSON.parse(wasm.evaluate_view(JSON.stringify(documentIr), JSON.stringify(currentView().segments), currentTime));
  $("graphic").applyOverrides(frame);
}

function drawTrack() {
  const canvas = $("track");
  const ctx = canvas.getContext("2d");
  const width = canvas.clientWidth || 800;
  canvas.width = width;
  canvas.height = 180;
  ctx.fillStyle = "#111";
  ctx.fillRect(0, 0, width, canvas.height);
  const rows = [];
  for (const segment of currentView().segments) {
    const board = documentIr.storyboards[segment.storyboard_index];
    (board?.animations || []).forEach((anim, index) => {
      rows.push({ anim, index, storyboardIndex: segment.storyboard_index, offset: segment.offset });
    });
  }
  rows.forEach((row, i) => {
    const y = 24 + i * 22;
    const x = 140 + (Number(row.anim.delay || 0) + row.offset) * pps;
    const w = Math.max(8, Number(row.anim.duration || 1) * pps);
    ctx.fillStyle = colorFor(row.anim.kind);
    ctx.fillRect(x, y, w, 16);
    ctx.fillStyle = "#fff";
    ctx.fillText(`${row.anim.kind} ${row.anim.object || ""}`, 8, y + 12);
    if (selectedAnim && selectedAnim.anim === row.anim) {
      ctx.strokeStyle = "#fff";
      ctx.strokeRect(x, y, w, 16);
    }
  });
  const playX = 140 + currentTime * pps;
  ctx.strokeStyle = "#f66";
  ctx.beginPath();
  ctx.moveTo(playX, 0);
  ctx.lineTo(playX, canvas.height);
  ctx.stroke();
}

function colorFor(kind) {
  return {
    Fade: "#4d9ae0",
    Fly: "#5cb85c",
    Reveal: "#d08b3c",
    ZoomFade: "#7d88dc",
    Rotate: "#c86c6c",
    Scroll: "#3c8bd0",
    Hidden: "#707070",
    RotateContinuous: "#c86c6c",
  }[kind] || "#888";
}

function hitTest(px, py) {
  const rows = [];
  for (const segment of currentView().segments) {
    const board = documentIr.storyboards[segment.storyboard_index];
    (board?.animations || []).forEach((anim, index) => {
      rows.push({ anim, index, storyboardIndex: segment.storyboard_index, offset: segment.offset });
    });
  }
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    const y = 24 + i * 22;
    const x = 140 + (Number(row.anim.delay || 0) + row.offset) * pps;
    const w = Math.max(8, Number(row.anim.duration || 1) * pps);
    if (px >= x && px <= x + w && py >= y && py <= y + 16) return row;
  }
  return null;
}

function selectObject(name) {
  selectedName = name || "";
  const object = objects().map(flattenObject).find((item) => item.name === selectedName);
  $("prop-name").value = selectedName;
  $("prop-text").value = object?.text || "";
  $("prop-x").value = object?.location?.x ?? 0;
  $("prop-y").value = object?.location?.y ?? 0;
  $("prop-w").value = object?.dimensions?.x ?? 0;
  $("prop-h").value = object?.dimensions?.y ?? 0;
  $("prop-visible").checked = object?.visible !== false;
}

function patchObject(mutator) {
  pushHistory();
  for (const layer of documentIr.layers || []) patchWalk(layer, mutator);
  renderStage();
}

function patchWalk(layer, mutator) {
  for (const child of layer.objects || []) {
    const object = flattenObject(child);
    if (object.name === selectedName) mutator(object);
    if (child.objects) patchWalk(child, mutator);
  }
}

async function openFile(file) {
  const buffer = await file.arrayBuffer();
  if (!wasm) throw new Error("wasm missing");
  if (file.name.endsWith(".json")) {
    documentIr = JSON.parse(await file.text());
  } else if (file.name.endsWith(".gtxml") || file.name.endsWith(".xml")) {
    documentIr = JSON.parse(wasm.parse_gtxml(await file.text()));
  } else {
    documentIr = JSON.parse(wasm.parse_gtzip(new Uint8Array(buffer)));
  }
  refreshStoryboardSelect();
  await renderStage();
}

$("file").addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file) return;
  try {
    await openFile(file);
    status(`Opened ${file.name}`);
  } catch (error) {
    status(String(error));
  }
});

$("graphic").addEventListener("gt-select", (event) => selectObject(event.detail));
$("prop-text").addEventListener("change", () => {
  const value = $("prop-text").value;
  if (wasm?.apply_field && selectedName) {
    documentIr = JSON.parse(wasm.apply_field(JSON.stringify(documentIr), `${selectedName}.Text`, value));
  } else {
    patchObject((object) => { object.text = value; });
  }
  renderStage();
});
for (const [id, key] of [["prop-x", "x"], ["prop-y", "y"]]) {
  $(id).addEventListener("change", () => {
    patchObject((object) => { object.location[key] = Number($(id).value); });
  });
}
for (const [id, key] of [["prop-w", "x"], ["prop-h", "y"]]) {
  $(id).addEventListener("change", () => {
    patchObject((object) => { object.dimensions[key] = Number($(id).value); });
  });
}
$("prop-visible").addEventListener("change", () => {
  patchObject((object) => { object.visible = $("prop-visible").checked; });
});
$("apply-ir").addEventListener("click", () => {
  documentIr = JSON.parse($("ir").value);
  refreshStoryboardSelect();
  renderStage();
});
$("export-gtzip").addEventListener("click", () => {
  if (!wasm?.write_gtzip_assets && !wasm?.write_gtzip) return status("wasm missing");
  const encoded = {};
  for (const [name, bytes] of Object.entries(assets)) {
    encoded[name] = bytesToB64(bytes);
  }
  const bytes = wasm.write_gtzip_assets
    ? wasm.write_gtzip_assets(JSON.stringify(documentIr), JSON.stringify(encoded))
    : wasm.write_gtzip(JSON.stringify(documentIr));
  download(new Blob([bytes]), "title.gtzip");
});
$("prop-image").addEventListener("change", async (event) => {
  const file = event.target.files?.[0];
  if (!file || !selectedName) return;
  const bytes = new Uint8Array(await file.arrayBuffer());
  const logical = `folder\\${file.name}`;
  assets[logical] = bytes;
  pushHistory();
  if (wasm?.apply_field) {
    documentIr = JSON.parse(wasm.apply_field(JSON.stringify(documentIr), `${selectedName}.Source`, logical));
    if (!objects().map(flattenObject).some((item) => item.name === selectedName && item.image_source === logical)) {
      documentIr = JSON.parse(wasm.apply_field(JSON.stringify(documentIr), `${selectedName}.Fill.Bitmap`, logical));
    }
  }
  renderStage();
});
document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === "z") {
    event.preventDefault();
    undo();
  }
});
$("export-html").addEventListener("click", async () => {
  if (!wasm?.to_html) return;
  const html = wasm.to_html(JSON.stringify(documentIr), "TransitionIn");
  download(new Blob([html], { type: "text/html" }), "title.html");
});
$("preview").addEventListener("change", () => {
  previewOn = $("preview").checked;
  seek(currentTime);
});
$("hold").addEventListener("change", () => {
  hold = Number($("hold").value);
  refreshStoryboardSelect();
  seek(currentTime);
});
$("zoom").addEventListener("input", () => {
  pps = Number($("zoom").value);
  drawTrack();
});
$("play").addEventListener("click", () => {
  if (playing) {
    playing = false;
    $("play").textContent = "Play";
    return;
  }
  previewOn = true;
  $("preview").checked = true;
  playing = true;
  playAnchor = performance.now();
  playStart = currentTime;
  $("play").textContent = "Pause";
  const tick = (now) => {
    if (!playing) return;
    currentTime = playStart + (now - playAnchor) / 1000;
    if (currentTime > viewDuration()) currentTime = 0, playAnchor = now, playStart = 0;
    seek(currentTime);
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
});
$("stop").addEventListener("click", () => {
  playing = false;
  $("play").textContent = "Play";
  seek(0);
});
$("add-anim").addEventListener("click", () => {
  if (!wasm?.edit_add_animation) return;
  pushHistory();
  if (!documentIr.storyboards?.length) {
    documentIr = JSON.parse(wasm.edit_add_storyboard(JSON.stringify(documentIr), "TransitionIn", ""));
  }
  const object = selectedName || objects()[0]?.name || "";
  const targetIndex = currentView().segments[0]?.storyboard_index ?? 0;
  const over = (documentIr.storyboards[targetIndex].animations || []).filter((anim) => anim.object === object && anim.kind !== "None").length;
  if (over >= 3) {
    $("tl-warn").textContent = `${object} already has 3 animations`;
    return;
  }
  $("tl-warn").textContent = "";
  documentIr = JSON.parse(wasm.edit_add_animation(JSON.stringify(documentIr), targetIndex, object, $("anim-type").value));
  renderStage();
});
$("del-anim").addEventListener("click", () => {
  if (!selectedAnim || !wasm?.edit_delete_animation) return;
  pushHistory();
  documentIr = JSON.parse(wasm.edit_delete_animation(JSON.stringify(documentIr), selectedAnim.storyboardIndex, selectedAnim.index));
  selectedAnim = null;
  renderStage();
});
for (const id of ["anim-delay", "anim-duration", "anim-reverse", "anim-ease", "anim-dir"]) {
  $(id).addEventListener("change", () => {
    if (!selectedAnim || !wasm?.edit_set_animation) return;
    pushHistory();
    const patch = {
      delay: $("anim-delay").value,
      duration: $("anim-duration").value,
      reversed: $("anim-reverse").checked,
      interpolation: $("anim-ease").value,
      direction: $("anim-dir").value,
    };
    documentIr = JSON.parse(wasm.edit_set_animation(JSON.stringify(documentIr), selectedAnim.storyboardIndex, selectedAnim.index, JSON.stringify(patch)));
    renderStage();
  });
}
$("track").addEventListener("pointerdown", (event) => {
  const rect = $("track").getBoundingClientRect();
  const x = event.clientX - rect.left;
  const y = event.clientY - rect.top;
  const hit = hitTest(x, y);
  if (hit) {
    selectedAnim = hit;
    $("anim-delay").value = hit.anim.delay || 0;
    $("anim-duration").value = hit.anim.duration || 1;
    $("anim-reverse").checked = !!hit.anim.reversed;
    $("anim-ease").value = hit.anim.interpolation || "Linear";
    $("anim-dir").value = hit.anim.direction || "Left";
    const barX = 140 + (Number(hit.anim.delay || 0) + hit.offset) * pps;
    const w = Math.max(8, Number(hit.anim.duration || 1) * pps);
    drag = {
      mode: x > barX + w - 8 ? "end" : x < barX + 8 ? "start" : "move",
      originX: x,
      delay: Number(hit.anim.delay || 0),
      duration: Number(hit.anim.duration || 1),
    };
  } else {
    previewOn = true;
    $("preview").checked = true;
    playing = false;
    seek(Math.max(0, (x - 140) / pps));
  }
  drawTrack();
});
$("track").addEventListener("pointermove", (event) => {
  if (!drag || !selectedAnim) return;
  const rect = $("track").getBoundingClientRect();
  const dx = (event.clientX - rect.left - drag.originX) / pps;
  if (drag.mode === "move") selectedAnim.anim.delay = String(Math.max(0, +(drag.delay + dx).toFixed(2)));
  if (drag.mode === "end") selectedAnim.anim.duration = String(Math.max(0.05, +(drag.duration + dx).toFixed(2)));
  if (drag.mode === "start") {
    const next = Math.max(0, drag.delay + dx);
    selectedAnim.anim.delay = String(next.toFixed(2));
    selectedAnim.anim.duration = String(Math.max(0.05, drag.duration - dx).toFixed(2));
  }
  drawTrack();
});
$("track").addEventListener("pointerup", () => {
  if (drag && selectedAnim && wasm?.edit_set_animation) {
    pushHistory();
    wasm.edit_set_animation(
      JSON.stringify(documentIr),
      selectedAnim.storyboardIndex,
      selectedAnim.index,
      JSON.stringify({ delay: selectedAnim.anim.delay, duration: selectedAnim.anim.duration }),
    );
  }
  drag = null;
});

function bytesToB64(bytes) {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

function download(blob, name) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

await loadWasm();
refreshStoryboardSelect();
drawTrack();
