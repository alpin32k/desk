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
}

export interface CustomAgent {
  id: string;
  command: string;
}

export interface AgentInfo {
  id: string;
  command: string;
  installed: boolean;
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
  openSession: (path: string, agent: string, instance: number) =>
    invoke<string>("open_session", { path, agent, instance }),
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
  unlockDesk: () => invoke<void>("unlock_desk"),
  isLocked: () => invoke<boolean>("is_locked"),
  openPreview: (url: string) => invoke<void>("open_preview", { url }),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  chatDir: () => invoke<string>("chat_dir"),
  gitBranches: (path: string) => invoke<string[]>("git_branches", { path }),
  gitSwitch: (path: string, branch: string) => invoke<void>("git_switch", { path, branch }),
  usage: (agents: string[]) => invoke<Usage[]>("usage", { agents }),
};
