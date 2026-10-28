// Typed wrappers around the Rust commands (src-tauri/src/lib.rs).
import { Channel, invoke } from "@tauri-apps/api/core";

export interface Project {
  path: string;
  name: string;
  lastActiveMs: number;
  isGit: boolean;
  worktreeOf: string | null;
  claudeSessions: number;
}

export interface Layout {
  version: number;
  lanes: string[][];
  autostart: string[];
  extraFolders: string[];
  notifications: boolean;
  colors: Record<string, number>;
  agents: CustomAgent[];
  projectsDir: string | null;
  /** Command of each ⌘U "run" session, by session key. */
  runs: Record<string, string>;
  /** Card of a session opened inside another project's card (⌘R conversations), by key. */
  cards: Record<string, string>;
  /** Columns follow what is open and the window size. */
  autoLayout: boolean;
  /** Color theme id (themes.ts). */
  theme: string;
  /** Web pages outside desk: "auto" (Brave, then Chrome), "brave", "chrome", "system". */
  browser: string;
  /** Terminal font size in px. */
  fontSize: number;
  /** Session keys whose tiles are folded to their header. */
  minimized: string[];
  /** Names given to tiles by hand, by session key. */
  titles: Record<string, string>;
  /** ⌘` slides the window down from the top of the screen. */
  dropdown: boolean;
  /** Height of the dropped-down window, percent of the screen. */
  dropdownHeight: number;
  /** The dropdown goes up when another app takes the focus. */
  dropdownAutohide: boolean;
  /** Its shortcut as an accelerator, "Super+Backquote" (hotkey.ts). */
  dropdownHotkey: string;
  /** How long it slides, ms (0: no slide). */
  dropdownAnimation: number;
  /** desk's shortcuts changed in Options, by binding id (keys.ts). */
  keys: Record<string, string>;
}

/** A newer desk release. */
export interface Update {
  version: string;
  url: string;
  /** desk can install it itself; else the release page opens. */
  installable: boolean;
}

/** Where the side panel's page sits in the window (CSS pixels). */
export interface PreviewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RunTarget {
  source: string;
  command: string;
}

export interface CustomAgent {
  id: string;
  command: string;
}

export interface AgentInfo {
  id: string;
  command: string;
  installed: boolean;
  /** Not installed, but desk installs it when a tile opens it. */
  installable: boolean;
  builtin: boolean;
}

export interface SessionInfo {
  slug: string;
  path: string;
  agent: string;
  instance: number;
}

export type HookState = "working" | "waiting" | "idle" | "exited";

export interface SessionState {
  slug: string;
  state: HookState;
  event: string;
  ts: number;
  message: string | null;
}

export interface Bootstrap {
  layout: Layout;
  sessions: SessionInfo[];
  states: SessionState[];
  home: string;
  /** macOS or Linux without tmux: sessions end when desk closes. */
  tmuxMissing: boolean;
}

export interface TileInfo {
  path: string;
  branch: string | null;
}

export type CoffeeLevel = "off" | "awake" | "lid";

export interface CoffeeStatus {
  level: CoffeeLevel;
  helperInstalled: boolean;
  sleepDisabled: boolean;
  stoppedBecause: string | null;
}

export interface UsageWindow {
  label: string;
  usedPercent: number;
  /** Unix seconds. */
  resetsAt: number;
}

export interface Usage {
  agent: string;
  windows: UsageWindow[];
}

export interface PtyExit {
  id: number;
  slug: string;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  listProjects: (extra: string[]) => invoke<Project[]>("list_projects", { extra }),
  listSessions: () => invoke<SessionInfo[]>("list_sessions"),
  openSession: (path: string, agent: string, instance: number, command?: string) =>
    invoke<string>("open_session", { path, agent, instance, command: command ?? null }),
  projectFiles: (path: string) => invoke<string[]>("project_files", { path }),
  openFile: (path: string, file: string) => invoke<void>("open_file", { path, file }),
  previewFile: (path: string, file: string, bounds: PreviewBounds) =>
    invoke<string>("preview_file", { path, file, bounds }),
  runTargets: (path: string) => invoke<RunTarget[]>("run_targets", { path }),
  listAgents: () => invoke<AgentInfo[]>("list_agents"),
  createProject: (parent: string, name: string, git: boolean) =>
    invoke<string>("create_project", { parent, name, git }),
  killSession: (slug: string) => invoke<void>("kill_session", { slug }),
  saveLayout: (layout: Layout) => invoke<void>("save_layout", { layout }),
  tileInfo: (paths: string[]) => invoke<TileInfo[]>("tile_info", { paths }),
  ptyAttach: (slug: string, cols: number, rows: number, onData: Channel<ArrayBuffer>) =>
    invoke<number>("pty_attach", { slug, cols, rows, onData }),
  ptyWrite: (id: number, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: number, cols: number, rows: number) => invoke<void>("pty_resize", { id, cols, rows }),
  ptyClose: (id: number) => invoke<void>("pty_close", { id }),
  coffeeStatus: () => invoke<CoffeeStatus>("coffee_status"),
  coffeeSet: (level: CoffeeLevel) => invoke<CoffeeStatus>("coffee_set", { level }),
  coffeeRestore: () => invoke<CoffeeStatus>("coffee_restore"),
  coffeeUninstall: () => invoke<CoffeeStatus>("coffee_uninstall"),
  lockDesk: () => invoke<void>("lock_desk"),
  /** `password`: the account password typed on the lock screen (Linux). */
  unlockDesk: (password?: string) => invoke<void>("unlock_desk", { password: password ?? null }),
  isLocked: () => invoke<boolean>("is_locked"),
  /** Register or drop ⌘` and refit the dropped-down window, as layout.json says. */
  dropdownApply: () => invoke<void>("dropdown_apply"),
  /** From the dropdown back to the ordinary window; the shortcut stays on. */
  dropdownLeave: () => invoke<void>("dropdown_leave"),
  /** Show a link in the side panel at `bounds`; resolves to what its address bar shows. */
  openPreview: (url: string, bounds: PreviewBounds) => invoke<string>("open_preview", { url, bounds }),
  previewBounds: (bounds: PreviewBounds) => invoke<void>("preview_bounds", { bounds }),
  previewVisible: (visible: boolean) => invoke<void>("preview_visible", { visible }),
  previewClose: () => invoke<void>("preview_close"),
  previewNav: (action: "back" | "forward" | "reload") => invoke<void>("preview_nav", { action }),
  /** Text of a file shown in the side panel, to edit it there. */
  readText: (path: string) => invoke<string>("read_text", { path }),
  /** Files of a project whose text contains `query`. */
  searchFiles: (path: string, query: string) => invoke<string[]>("search_files", { path, query }),
  /** Show a file or folder in Finder / Explorer / the file manager. */
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  writeText: (path: string, text: string) => invoke<void>("write_text", { path, text }),
  /** A web page in Brave / Chrome / the system browser (`browser`, see Layout), a file in its default app. */
  openExternal: (url: string, browser?: string) => invoke<void>("open_external", { url, browser }),
  /** "Brave" or "Chrome" when that is where web pages go; null = the system browser. */
  browserName: (browser: string) => invoke<string | null>("browser_name", { browser }),
  updateCheck: () => invoke<Update | null>("update_check"),
  /** Downloads `version`, starts it and quits desk. */
  updateInstall: (version: string) => invoke<void>("update_install", { version }),
  chatDir: () => invoke<string>("chat_dir"),
  gitBranches: (path: string) => invoke<string[]>("git_branches", { path }),
  gitSwitch: (path: string, branch: string) => invoke<void>("git_switch", { path, branch }),
  usage: (agents: string[]) => invoke<Usage[]>("usage", { agents }),
};
