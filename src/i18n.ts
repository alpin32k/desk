// Interface language (Options → Ogólne / General). The first launch takes the
// system's language; the choice is kept in layout.language. Messages live in
// locales/: pl.ts sets the keys, every other language must have them all.
import { en } from "./locales/en";
import { pl, type Messages } from "./locales/pl";

export type Language = "pl" | "en";
export type MessageKey = keyof Messages;

/** In the order Options lists them, each named in its own language. */
export const LANGUAGES: { id: Language; name: string }[] = [
  { id: "pl", name: "Polski" },
  { id: "en", name: "English" },
];

const MESSAGES: Record<Language, Messages> = { pl, en };

let current: Language = "pl";

export function isLanguage(id: string): id is Language {
  return id in MESSAGES;
}

export function language(): Language {
  return current;
}

export function setLanguage(id: Language): void {
  current = id;
  if (typeof document !== "undefined") document.documentElement.lang = id;
}

/** The first of the system's preferred languages desk speaks; English when none. */
export function detectLanguage(preferred: readonly string[] = navigator.languages ?? [navigator.language]): Language {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split(/[-_]/)[0];
    if (isLanguage(base)) return base;
  }
  return "en";
}

/** The message in the current language, `{name}` replaced by `params.name`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const text = MESSAGES[current][key];
  return params ? text.replace(/\{(\w+)\}/g, (m: string, k: string) => (k in params ? String(params[k]) : m)) : text;
}

/** A message with plural forms split by "|" (Polish: one|few|many, English:
 *  one|other): the form for `n`, with `{n}` filled in. */
export function tn(key: MessageKey, n: number, params?: Record<string, string | number>): string {
  const forms = t(key, { ...params, n }).split("|");
  return forms[Math.min(pluralForm(current, n), forms.length - 1)];
}

function pluralForm(lang: Language, n: number): number {
  if (n === 1) return 0;
  if (lang === "en") return 1;
  const ones = n % 10;
  const tens = n % 100;
  return ones >= 2 && ones <= 4 && (tens < 12 || tens > 14) ? 1 : 2;
}
