import { StringStream } from "@codemirror/language";
import { describe, expect, it } from "vitest";
import { dotenvParser, isDotenv } from "./dotenv";

/** Each non-blank piece of `text` with the style the parser gives it. */
function tokens(text: string): [string, string | null][] {
  const state = dotenvParser.startState!(2);
  const out: [string, string | null][] = [];
  for (const line of text.split("\n")) {
    const s = new StringStream(line, 2, 2);
    while (!s.eol()) {
      const style = dotenvParser.token(s, state);
      if (s.current().trim()) out.push([s.current(), style ?? null]);
      s.start = s.pos;
    }
  }
  return out;
}

describe(".env files", () => {
  it("are told by name, extension or not", () => {
    expect([".env", ".env.docker", ".env.local", ".env.example", "prod.env"].map(isDotenv)).toEqual([true, true, true, true, true]);
    expect([".envrc", "env.ts", "environment.md", "a.env.bak"].map(isDotenv)).toEqual([false, false, false, false]);
  });

  it("color keys, values and comments", () => {
    expect(tokens("# db\nexport DB_URL=postgres://x # local\nKEY = 'a b'\nEMPTY=")).toEqual([
      ["# db", "comment"],
      ["export", "keyword"],
      ["DB_URL", "propertyName"],
      ["=", "operator"],
      ["postgres://x", "string"],
      ["# local", "comment"],
      ["KEY", "propertyName"],
      ["=", "operator"],
      ["'a b'", "string"],
      ["EMPTY", "propertyName"],
      ["=", "operator"],
    ]);
  });

  it("keep a # inside quotes and a quoted value over several lines", () => {
    expect(tokens('HASH="a#b" # c\nKEY="-----BEGIN\nabc\n-----END"\nNEXT=1')).toEqual([
      ["HASH", "propertyName"],
      ["=", "operator"],
      ['"a#b"', "string"],
      ["# c", "comment"],
      ["KEY", "propertyName"],
      ["=", "operator"],
      ['"-----BEGIN', "string"],
      ["abc", "string"],
      ['-----END"', "string"],
      ["NEXT", "propertyName"],
      ["=", "operator"],
      ["1", "string"],
    ]);
  });

  it("mark references to other variables", () => {
    expect(tokens("URL=http://${HOST}:$PORT/x")).toEqual([
      ["URL", "propertyName"],
      ["=", "operator"],
      ["http://", "string"],
      ["${HOST}", "variableName.special"],
      [":", "string"],
      ["$PORT", "variableName.special"],
      ["/x", "string"],
    ]);
  });
});
