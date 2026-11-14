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

/** Sessions of a card one under another, side by side, or one at a time. */
export type CardMode = "stack" | "row" | "tabs";

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
  /** desk places new projects (a lane of their own while there is room) and drops empty lanes. */
  autoLayout: boolean;
  /** Width of each lane, as a weight (index = lane). */
  widths: number[];
  /** Height of a project's card in its lane, as a weight, by card. */
  heights: Record<string, number>;
  /** Room of a session in its card, as a weight, by session key. */
  sizes: Record<string, number>;
  /** How a card shows its sessions, by card: "row" side by side, "tabs" one at a time. */
  modes: Record<string, CardMode>;
  /** The session a card in tabs shows, by card. */
  tabs: Record<string, string>;
  /** Color theme id (themes.ts). */
  theme: string;
  /** Web pages outside desk: "auto" (Brave, then Chrome), "brave", "chrome", "system". */
  browser: string;
  /** A plain click on a terminal link: "desk", "browser" or "none" (links.ts). */
  linkClick: string;
  /** Terminal font size in px. */
  fontSize: number;
  /** Session keys whose tiles are folded to their header. */
  minimized: string[];
  /** Names given to tiles by hand, by session key. */
  titles: Record<string, string>;
  /** Claude conversations saved under a name, by project path, then by conversation id (saved.ts). */
  saved: Record<string, Record<string, string>>;
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
  /** A right click in a terminal opens tmux's menu (split, swap, kill...). */
  tmuxMenu: boolean;
  /** Interface language, "pl" or "en" (i18n.ts); "" until the first launch picks one. */
  language: string;
  /** Ctrl+G in Claude rewrites the prompt with the company LLM (prompt_ai.rs). */
  promptAi: PromptAi;
  /** Claude accounts (accounts.ts); the first one is the default. */
  claudeAccounts: ClaudeAccount[];
  /** The account each Claude session was started with, by session key. */
  tileAccounts: Record<string, string>;
  /** The account new Claude sessions of a project use, by project path. */
  projectAccounts: Record<string, string>;
  /** What desk does when hot (thermal.ts, thermal.rs Mode). */
  thermalMode: ThermalMode;
  /** Slow the sessions while the CPU is hotter than this, °C (thermal.ts). */
  thermalLimit: number;
  /** Fans at full speed from this, °C, in mode "fansFirst" (fans.rs). */
  fanLimit: number;
}

/** What desk does when hot: nothing, slow its sessions, the fans at full speed, or both. */
export type ThermalMode = "off" | "slow" | "fansFirst" | "together" | "fans";

/** A Claude Code config folder with its own login (accounts.rs). */
export interface ClaudeAccount {
  id: string;
  name: string;
  /** As typed in Options ("~/.claude-prywatne"); "" = Claude Code's default. */
  dir: string;
}

/** Where an account lives and who is logged in on it. */
export interface AccountInfo {
  id: string;
  dir: string;
  email: string | null;
}

export interface PromptAi {
  enabled: boolean;
  /** OpenAI-compatible server: base, /v1 or the full chat endpoint. */
  url: string;
  model: string;
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

/** A GitHub repo the user can clone (`gh` login). */
export interface GithubRepo {
  fullName: string;
  description: string | null;
  private: boolean;
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
  /** The Claude Code conversation in the tile. */
  sessionId: string | null;
}

/** A Claude Code conversation held in a project (⌘S). */
export interface Conversation {
  id: string;
  /** From `/rename`, else the one Claude Code gave it. */
  title: string | null;
  /** The first message typed in it, on one line. */
  prompt: string;
  branch: string | null;
  modifiedMs: number;
  /** The Claude account it was held on; it resumes there. */
  account: string;
}

export interface Bootstrap {
  layout: Layout;
  sessions: SessionInfo[];
  states: SessionState[];
  home: string;
  /** Where "new project" suggests creating folders until one is chosen. */
  projectsDir: string;
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

/** CPU temperature and desk slowing its sessions while hot (thermal.rs). */
export interface ThermalStatus {
  /** Hottest CPU sensor, °C; null when desk cannot read one here. */
  celsius: number | null;
  /** Slowing limit; 0: this mode never slows. */
  limit: number;
  /** Fan limit; 0: this mode never uses the fans. */
  fanLimit: number;
  /** Too hot: desk is slowing its sessions. */
  slowed: boolean;
  /** Processes slowed (the sessions and everything they started). */
  processes: number;
  /** Why desk cannot slow anything here, if it cannot. */
  cannotSlow: string | null;
  /** The fans, when the mode uses them. */
  fans: FanStatus | null;
}

/** The fans while a mode uses them (fans.rs). */
export interface FanStatus {
  /** desk has them at full speed. */
  forced: boolean;
  /** The fastest fan now. */
  rpm: number | null;
  maxRpm: number | null;
  /** Why desk is not driving them while it wants to. */
  problem: string | null;
}

/** For Options: can desk drive this Mac's fans, and is the permission installed. */
export interface FanInfo {
  /** Why the fan modes are greyed out, if they are. */
  unsupported: string | null;
  consent: "missing" | "outdated" | "installed";
}

export interface UsageWindow {
  label: string;
  usedPercent: number;
  /** Unix seconds. */
  resetsAt: number;
}

export interface Usage {
  agent: string;
  /** The Claude account, with two or more of them. */
  account: string | null;
  windows: UsageWindow[];
}

export interface PtyExit {
  id: number;
  slug: string;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  /** Server of the saved prompt-rewriting VPN config, null when none (the key never leaves Rust). */
  promptAiVpn: () => invoke<string | null>("prompt_ai_vpn"),
  /** Save a pasted WireGuard .conf ("" removes it); returns its server. */
  setPromptAiVpn: (conf: string) => invoke<string | null>("set_prompt_ai_vpn", { conf }),
  listProjects: (extra: string[]) => invoke<Project[]>("list_projects", { extra }),
  listSessions: () => invoke<SessionInfo[]>("list_sessions"),
  /** `resume`: a Claude conversation's id, to pick it up again (⌘S). */
  /** `restore`: a tile of the saved layout at startup; Claude resumes its last conversation if the session is gone. */
  /** `account`: the Claude account (accounts.ts) a new Claude session starts on. */
  openSession: (path: string, agent: string, instance: number, command?: string, resume?: string, restore = false, account?: string) =>
    invoke<string>("open_session", {
      path,
      agent,
      instance,
      command: command ?? null,
      resume: resume ?? null,
      restore,
      account: account || null,
    }),
  /** The project's newest Claude conversations (⌘S). */
  /** `saved`: ids of the project's saved conversations, listed however old. */
  recentConversations: (path: string, saved: string[]) => invoke<Conversation[]>("recent_conversations", { path, saved }),
  projectFiles: (path: string) => invoke<string[]>("project_files", { path }),
  /** One folder of a project: files, and folders as `rel/name/`. */
  projectDir: (path: string, rel: string) => invoke<string[]>("project_dir", { path, rel }),
  openFile: (path: string, file: string) => invoke<void>("open_file", { path, file }),
  previewFile: (path: string, file: string, bounds: PreviewBounds) =>
    invoke<string>("preview_file", { path, file, bounds }),
  runTargets: (path: string) => invoke<RunTarget[]>("run_targets", { path }),
  listAgents: () => invoke<AgentInfo[]>("list_agents"),
  createProject: (parent: string, name: string, git: boolean) =>
    invoke<string>("create_project", { parent, name, git }),
  githubRepos: () => invoke<GithubRepo[]>("github_repos"),
  cloneProject: (parent: string, name: string, repo: string) =>
    invoke<string>("clone_project", { parent, name, repo }),
  killSession: (slug: string) => invoke<void>("kill_session", { slug }),
  saveLayout: (layout: Layout) => invoke<void>("save_layout", { layout }),
  tileInfo: (paths: string[]) => invoke<TileInfo[]>("tile_info", { paths }),
  ptyAttach: (slug: string, cols: number, rows: number, onData: Channel<ArrayBuffer>) =>
    invoke<number>("pty_attach", { slug, cols, rows, onData }),
  ptyWrite: (id: number, data: string) => invoke<void>("pty_write", { id, data }),
  /** Where the files of the last drag are (macOS; empty elsewhere). */
  droppedPaths: () => invoke<string[]>("dropped_paths"),
  ptyResize: (id: number, cols: number, rows: number) => invoke<void>("pty_resize", { id, cols, rows }),
  ptyClose: (id: number) => invoke<void>("pty_close", { id }),
  thermalStatus: () => invoke<ThermalStatus>("thermal_status"),
  fansInfo: () => invoke<FanInfo>("fans_info"),
  /** The one admin password prompt for the fans. */
  fansInstall: () => invoke<FanInfo>("fans_install"),
  fansUninstall: () => invoke<FanInfo>("fans_uninstall"),
  coffeeStatus: () => invoke<CoffeeStatus>("coffee_status"),
  coffeeSet: (level: CoffeeLevel) => invoke<CoffeeStatus>("coffee_set", { level }),
  coffeeRestore: () => invoke<CoffeeStatus>("coffee_restore"),
  coffeeUninstall: () => invoke<CoffeeStatus>("coffee_uninstall"),
  lockDesk: () => invoke<void>("lock_desk"),
  /** `password`: the account password typed on the lock screen (Linux). */
  unlockDesk: (password?: string) => invoke<void>("unlock_desk", { password: password ?? null }),
  isLocked: () => invoke<boolean>("is_locked"),
  /** Quit desk (the page asks first); refused while locked. */
  quitDesk: () => invoke<void>("quit_desk"),
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
  /** Each Claude account's folder and login (Options). */
  claudeAccounts: () => invoke<AccountInfo[]>("claude_accounts"),
};
