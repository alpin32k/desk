// `.env` files (.env, .env.local, .env.docker, prod.env) for the side panel's
// editor: KEY=value lines, # comments, an optional `export`. CodeMirror picks
// a language by extension, and `.env.docker` has none it knows.

import { StreamLanguage, type StreamParser, type StringStream } from "@codemirror/language";

/** `.env`, `.env.<anything>` or `<name>.env`. */
export function isDotenv(fileName: string): boolean {
  return /^\.env(\.[^.]+)*$|^[^.]+\.env$/i.test(fileName);
}

interface State {
  /** Past the `=` of this line. */
  value: boolean;
  /** Inside a quoted value (it can go on for several lines). */
  quote: string | null;
}

/** The rest of a quoted value on this line. */
function quoted(stream: StringStream, state: State): string {
  while (!stream.eol()) {
    const ch = stream.next();
    if (ch === "\\") stream.next();
    else if (ch === state.quote) {
      state.quote = null;
      break;
    }
  }
  return "string";
}

export const dotenvParser: StreamParser<State> = {
  name: "dotenv",
  startState: () => ({ value: false, quote: null }),
  copyState: (s) => ({ ...s }),
  token(stream, state) {
    if (state.quote) return quoted(stream, state);
    if (stream.sol()) state.value = false;
    if (stream.eatSpace()) return null;
    if (!state.value) {
      if (stream.match("#")) {
        stream.skipToEnd();
        return "comment";
      }
      if (stream.match(/^export(?=\s)/)) return "keyword";
      if (stream.match(/^[^=\s]+/)) return "propertyName";
      stream.next(); // the "="
      state.value = true;
      return "operator";
    }
    // Outside quotes a # starts a comment, as dotenv reads it.
    if (stream.match("#")) {
      stream.skipToEnd();
      return "comment";
    }
    const ch = stream.peek()!;
    if (`"'\``.includes(ch) && stream.string.slice(0, stream.pos).trimEnd().endsWith("=")) {
      stream.next();
      state.quote = ch;
      return quoted(stream, state);
    }
    if (stream.match(/^\$\{[^}\s]*\}?|^\$\w+/)) return "variableName.special";
    if (!stream.match(/^[^\s#$]+/)) stream.next();
    return "string";
  },
};

export const dotenv = StreamLanguage.define(dotenvParser);
