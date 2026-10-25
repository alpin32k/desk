# desk

Native macOS app (Tauri 2) showing a grid of live Claude Code sessions, one per project.
Sessions run in tmux on a private socket (`tmux -L desk`) and survive closing the app.
Session state (working / waiting / idle) comes from Claude Code hooks injected with
`claude --settings`, so the global Claude Code and tmux configs are never touched.

```bash
pnpm install
pnpm tauri dev          # run in development
pnpm check              # typecheck, eslint, vitest, clippy, cargo test
pnpm tauri build        # -> src-tauri/target/release/bundle/macos/desk.app
```

Data lives in `~/Library/Application Support/desk/` (layout.json, state/, hook settings, tmux.conf).
