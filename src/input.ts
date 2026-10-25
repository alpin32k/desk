// Keyboard input from xterm.js on its way to the session.
//
// When tmux attaches it asks the outer terminal (xterm.js) for its Device
// Attributes. xterm.js answers ("\x1b[?1;2c", "\x1b[>0;276;0c") through the
// same input stream, and tmux sometimes takes a late answer for typed keys:
// "c?1;2c" showed up in Claude's prompt after desk restarted. Programs inside
// the session never see these queries (tmux answers them itself), so the
// replies can be dropped.
// eslint-disable-next-line no-control-regex -- matching ESC is the point
const DA_REPLY = /\x1b\[[?>][\d;]*c/g;

export function stripProbeReplies(data: string): string {
  return data.replace(DA_REPLY, "");
}
