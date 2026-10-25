// The side panel's address bar.

/** What was typed in the address bar: a path, a URL, or a bare host. */
export function normalize(input: string): string {
  const s = input.trim();
  if (!s || /^[a-z][a-z0-9+.-]*:\/\//i.test(s) || s.startsWith("/") || s.startsWith("~") || /^[a-z]:[\\/]/i.test(s)) {
    return s;
  }
  return /^(localhost|127\.|\[::1\])/.test(s) ? `http://${s}` : `https://${s}`;
}
