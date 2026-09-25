import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

interface SquadEntry {
  path: string;
  name: string;
}

interface OperatorInfo {
  operator: string;
  window: string;
}

const viewSquads = document.querySelector<HTMLElement>("#view-squads")!;
const viewGrid = document.querySelector<HTMLElement>("#view-grid")!;
const squadGrid = document.querySelector<HTMLElement>("#squad-grid")!;
const termGrid = document.querySelector<HTMLElement>("#terminal-grid")!;
const gridTitle = document.querySelector<HTMLElement>("#grid-title")!;
const addSquadBtn = document.querySelector<HTMLButtonElement>("#add-squad-btn")!;
const backBtn = document.querySelector<HTMLButtonElement>("#back-btn")!;

// Active terminals for the currently open squad, so we can tear them down
// cleanly (kill the PTY, unsubscribe events) when navigating back.
let activeTiles: { id: string; unlisten: UnlistenFn[]; term: Terminal }[] = [];

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

async function loadSquads() {
  const squads = await invoke<SquadEntry[]>("list_squads");
  squadGrid.innerHTML = "";
  for (const squad of squads) {
    const card = document.createElement("button");
    card.className = "card squad-card";
    card.innerHTML = `<span class="name">${squad.name}</span><span class="path">${squad.path}</span>`;
    card.addEventListener("click", () => openSquad(squad));
    squadGrid.appendChild(card);
  }
}

addSquadBtn.addEventListener("click", async () => {
  const selected = await open({ directory: true, multiple: false });
  if (!selected || typeof selected !== "string") return;
  try {
    await invoke("add_squad", { path: selected });
    await loadSquads();
  } catch (err) {
    alert(String(err));
  }
});

backBtn.addEventListener("click", () => closeSquad());

async function openSquad(squad: SquadEntry) {
  gridTitle.textContent = squad.name;
  viewSquads.hidden = true;
  viewGrid.hidden = false;
  termGrid.innerHTML = "";
  activeTiles = [];

  const operators = await invoke<OperatorInfo[]>("list_operators", {
    squadName: squad.name,
  });

  if (operators.length === 0) {
    termGrid.innerHTML = `<p class="muted">No operators are running for this squad right now.</p>`;
    return;
  }

  for (const op of operators) {
    await spawnTile(op);
  }
}

function closeSquad() {
  for (const tile of activeTiles) {
    tile.unlisten.forEach((fn) => fn());
    tile.term.dispose();
    invoke("close_terminal", { id: tile.id }).catch(() => {});
  }
  activeTiles = [];
  viewGrid.hidden = true;
  viewSquads.hidden = false;
}

async function spawnTile(op: OperatorInfo) {
  const id = `${op.window}-${Date.now()}`;

  const tile = document.createElement("div");
  tile.className = "term-tile";
  tile.innerHTML = `
    <div class="term-head">
      <span class="label">${op.operator}</span>
    </div>
    <div class="term-body"></div>
  `;
  termGrid.appendChild(tile);
  const body = tile.querySelector<HTMLElement>(".term-body")!;

  const term = new Terminal({
    fontFamily: cssVar("--font-mono"),
    fontSize: 13,
    theme: {
      background: cssVar("--surface"),
      foreground: cssVar("--fg"),
      cursor: cssVar("--accent"),
      selectionBackground: cssVar("--selection"),
      red: cssVar("--red"),
      green: cssVar("--green"),
      yellow: cssVar("--yellow"),
      blue: cssVar("--blue"),
      magenta: cssVar("--magenta"),
      cyan: cssVar("--cyan"),
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(body);
  fit.fit();

  const unlistenData = await listen<string>(`pty-data-${id}`, (event) => {
    term.write(event.payload);
  });
  const unlistenClosed = await listen(`pty-closed-${id}`, () => {
    term.write("\r\n\x1b[2m[session ended]\x1b[0m\r\n");
  });

  term.onData((data) => {
    invoke("write_terminal", { id, data }).catch(() => {});
  });

  await invoke("spawn_terminal", {
    id,
    window: op.window,
    rows: term.rows,
    cols: term.cols,
  });

  const resizeObserver = new ResizeObserver(() => {
    fit.fit();
    invoke("resize_terminal", { id, rows: term.rows, cols: term.cols }).catch(() => {});
  });
  resizeObserver.observe(body);

  activeTiles.push({
    id,
    term,
    unlisten: [unlistenData, unlistenClosed, () => resizeObserver.disconnect()],
  });
}

loadSquads();
