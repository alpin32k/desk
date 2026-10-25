# Claude Code hooks used by desk

Verified 2026-10-25 against Claude Code 2.1.310 (`claude --settings <file> -p ... --debug=hooks`).

- `claude --settings <file-or-json>` loads *additional* settings. Hooks from that file
  are merged with the user's own hooks (user `~/.claude/settings.json` and plugin hooks
  still ran in the same session), so desk never edits the global config.
- Observed events in a `-p` run: `SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd`.
  Each receives JSON on stdin with at least `session_id`, `transcript_path`, `cwd`
  (plus `hook_event_name`).
- `Notification` and `PermissionRequest` only fire interactively. `Notification`
  carries `notification_type` (`permission_prompt`, `idle_prompt`, `elicitation_dialog`, ...).

## State mapping

| Event | State |
|---|---|
| `SessionStart`, `Stop`, `SessionEnd` | idle |
| `UserPromptSubmit`, `PostToolUse` | working |
| `PermissionRequest`, `Notification` (except `idle_prompt`) | waiting |
| `Notification` with `idle_prompt` | unchanged (Claude already finished; still idle) |

`PostToolUse` is there so a tile leaves "waiting" once a permission prompt is answered.
