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
}

export interface SessionInfo {
  slug: string;
  path: string;
  command: string;
}

export type HookState = "working" | "waiting" | "idle";

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
  command: string | null;
}

export interface PtyExit {
  id: number;
  slug: string;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  listProjects: (extra: string[]) => invoke<Project[]>("list_projects", { extra }),
  listSessions: () => invoke<SessionInfo[]>("list_sessions"),
  openSession: (path: string) => invoke<string>("open_session", { path }),
  killSession: (slug: string) => invoke<void>("kill_session", { slug }),
  saveLayout: (layout: Layout) => invoke<void>("save_layout", { layout }),
  tileInfo: (paths: string[]) => invoke<TileInfo[]>("tile_info", { paths }),
  ptyAttach: (slug: string, cols: number, rows: number, onData: Channel<ArrayBuffer>) =>
    invoke<number>("pty_attach", { slug, cols, rows, onData }),
  ptyWrite: (id: number, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: number, cols: number, rows: number) => invoke<void>("pty_resize", { id, cols, rows }),
  ptyClose: (id: number) => invoke<void>("pty_close", { id }),
};
