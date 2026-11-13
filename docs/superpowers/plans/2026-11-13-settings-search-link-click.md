# Opcje z szukajką i klik w link: plan wdrożenia

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ustawienie „Klik w link otwiera" (desk / przeglądarka / nic) oraz okno Opcji z paskiem bocznym ośmiu grup i szukajką.

**Architecture:** Dwie czyste funkcje z testami (`linkTarget` w `src/links.ts`, `search` w nowym `src/settings-search.ts`) niosą całą logikę; `tile.ts` i `settings.ts` tylko je wołają. Modal dalej budowany jednym `render()` przez `innerHTML`, a szukanie przełącza `hidden` na gotowych elementach bez ponownego renderu.

**Tech Stack:** TypeScript, xterm.js, Vite, Vitest (środowisko node, bez DOM), Tauri 2 / Rust (serde).

**Spec:** `docs/superpowers/specs/2026-11-13-settings-search-link-click-design.md`

## Global Constraints

- Domyślne zachowanie bez zmian: brak `linkClick` w `layout.json` albo nieznana wartość = `"desk"`.
- Żadna istniejąca opcja nie zmienia znaczenia ani klucza w `layout.json`.
- Przycisk ↗, pasek adresu, ⌘F, ⌥↵ bez zmian. `altClickMovesCursor: false` zostaje.
- Kod, komentarze i identyfikatory po angielsku; teksty interfejsu w `src/locales/pl.ts` i `src/locales/en.ts` (oba zawsze razem).
- Nazwy klawiszy w tekstach przez istniejące pomocniki (`keysIn`, `shortcut`), nie na sztywno.
- Commity: tytuł po angielsku jako zdanie o zachowaniu (jak w `git log`), treść po polsku. Bez `CHANGELOG.md` (dopisywany w commicie wydania).
- Przed każdym commitem: `pnpm typecheck && pnpm lint && pnpm test`; po zmianach w Rust także `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`.

## Review Focus

1. Zapytanie bez polskich znaków („przegladarka", „jezyk", „wyglad", „lacze") znajduje tekst z ogonkami; `ł` nie rozkłada się przez NFD. Test w zadaniu 3.
2. ⌥ i ⌘ wciśnięte naraz, oraz nieznana wartość trybu z ręcznie edytowanego `layout.json`. Testy w zadaniu 1.
3. Zapytanie z samych spacji zachowuje się jak puste (widać bieżącą grupę, bez liczników). Test w zadaniu 3.
4. Stary `layout.json` bez `linkClick` wczytuje się z `"desk"`. Test Rust w zadaniu 2.
5. `render()` wywołany w trakcie szukania (zmiana opcji, doładowanie projektów) nie gubi zapytania, wyniku ani fokusu szukajki. Brak DOM w testach: sprawdzane ręcznie w zadaniu 5, krok 4.

---

### Task 1: Reguła celu linku

**Files:**
- Modify: `src/links.ts` (na końcu pliku)
- Test: `src/links.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type LinkClick = "desk" | "browser" | "none";
  export type LinkTarget = "desk" | "browser";
  /** `alt`: Option/Alt held; `mod`: ⌘ on macOS, Ctrl elsewhere. */
  export function linkTarget(mode: string, keys: { alt: boolean; mod: boolean }): LinkTarget | null;
  ```

- [ ] **Step 1: Testy (mają nie przechodzić)** w `src/links.test.ts`, `describe("linkTarget")`:

```ts
const none = { alt: false, mod: false }, alt = { alt: true, mod: false }, mod = { alt: false, mod: true }, both = { alt: true, mod: true };
it("desk mode: click opens in desk, alt-click in the browser", () => {
  expect(linkTarget("desk", none)).toBe("desk");
  expect(linkTarget("desk", alt)).toBe("browser");
  expect(linkTarget("desk", mod)).toBe("desk");
});
it("browser mode: the other way round", () => {
  expect(linkTarget("browser", none)).toBe("browser");
  expect(linkTarget("browser", alt)).toBe("desk");
  expect(linkTarget("browser", mod)).toBe("browser");
});
it("none mode: a plain click does nothing", () => {
  expect(linkTarget("none", none)).toBeNull();
  expect(linkTarget("none", alt)).toBe("browser");
  expect(linkTarget("none", mod)).toBe("desk");
});
it("alt wins over mod", () => {
  expect(linkTarget("desk", both)).toBe("browser");
  expect(linkTarget("browser", both)).toBe("desk");
  expect(linkTarget("none", both)).toBe("browser");
});
it("an unknown mode is desk", () => {
  expect(linkTarget("", none)).toBe("desk");
  expect(linkTarget("whatever", alt)).toBe("browser");
});
```

- [ ] **Step 2:** `pnpm vitest run src/links.test.ts` → FAIL (`linkTarget` nie istnieje).
- [ ] **Step 3:** Dodaj typy i `linkTarget` do `src/links.ts`.
- [ ] **Step 4:** `pnpm vitest run src/links.test.ts` → PASS.
- [ ] **Step 5:** Commit: `A link's target follows a click mode and the modifier held`.

---

### Task 2: Pole `linkClick` w układzie i klik w terminalu

**Files:**
- Modify: `src-tauri/src/layout.rs` (pole obok `browser`, `Default`, test)
- Modify: `src/api.ts` (`Layout`, obok `browser: string`)
- Modify: `src/app.ts` (kontekst `Side` ok. linii 152, kontekst `Settings` ok. 197, wartość startowa ok. 334)
- Modify: `src/side.ts` (`SideContext`, eksport)
- Modify: `src/settings.ts` (`SettingsContext`)
- Modify: `src/tile.ts` (`openLink` ok. linii 232, opcje `new Terminal` ok. 182)

**Interfaces:**
- Consumes: `linkTarget`, `LinkClick` z zadania 1.
- Produces:
  - Rust: `pub link_click: String` w `Layout`, domyślnie `"desk"`; w JSON `linkClick` (jak pozostałe pola).
  - `Layout.linkClick: string` w `src/api.ts`.
  - `SideContext.linkClick(): string`; w `src/side.ts`: `export function linkClickMode(): string` (zwraca `"desk"`, gdy nie ma `Side.current`).
  - `SettingsContext.setLinkClick: (mode: LinkClick) => void` (zapisuje układ tak jak `setBrowser`).

- [ ] **Step 1: Test Rust (ma nie przechodzić)** w module testów `layout.rs`: JSON układu bez `linkClick` deserializuje się z `link_click == "desk"`; z `"linkClick":"none"` daje `"none"`. Wzoruj się na istniejących testach w tym pliku.
- [ ] **Step 2:** `cd src-tauri && cargo test layout` → FAIL.
- [ ] **Step 3:** Dodaj pole z komentarzem doc (`"desk"`, `"browser"` or `"none"`: what a plain click on a terminal link does, see src/links.ts) i wartość w `Default`.
- [ ] **Step 4:** `cargo test layout` → PASS.
- [ ] **Step 5:** Przeprowadź pole przez `api.ts`, `app.ts`, `side.ts`, `settings.ts` według Interfaces. `setLinkClick` w `app.ts` ustawia `this.layout.linkClick` i zapisuje układ tą samą drogą co `setBrowser`.
- [ ] **Step 6:** W `tile.ts` przepisz `openLink`: `const target = linkTarget(linkClickMode(), { alt: e.altKey, mod: PLATFORM === "mac" ? e.metaKey : e.ctrlKey })`; `null` → `return` bez żadnego efektu; `"browser"` → dzisiejsza gałąź `openOutside`; `"desk"` → dzisiejsza gałąź `preview` / `previewFile`. Tryb jest czytany przy każdym kliknięciu.
- [ ] **Step 7:** W opcjach `new Terminal` dodaj `linkHandler: { activate: (e, uri) => openLink(e, uri) }`, żeby hiperłącza OSC 8 szły tą samą drogą. `openLink` trzeba zdefiniować przed konstruktorem terminala albo podpiąć przez `this.term.options.linkHandler` zaraz po nim; błąd z `openLink` dalej wypisuje się w terminalu jak dziś.
- [ ] **Step 8:** `pnpm typecheck && pnpm lint && pnpm test`, potem `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test` → wszystko zielone.
- [ ] **Step 9:** Commit: `Terminal links follow the "click opens" setting, OSC 8 links too`.

Po tym zadaniu tryb da się ustawić tylko ręcznie w `layout.json`; kontrolka dochodzi w zadaniu 4.

---

### Task 3: Dopasowanie szukajki

**Files:**
- Create: `src/settings-search.ts`
- Test: `src/settings-search.test.ts`

**Interfaces:**
- Produces:
  ```ts
  /** Lower case, no diacritics (ł too), single spaces, trimmed. */
  export function normalize(text: string): string;
  /** One thing Options can show or hide: a line, a shortcut, or a whole section. */
  export interface SearchItem { group: string; section: string; text: string }
  export interface SearchResult {
    /** Same order as the items. */
    show: boolean[];
    /** Shown items per group id; empty when there is no query. */
    counts: Record<string, number>;
    /** The query after normalize(); "" = not searching. */
    query: string;
  }
  /** `groupNames`: group id -> its name in the sidebar. */
  export function search(items: SearchItem[], groupNames: Record<string, string>, query: string): SearchResult;
  ```
- Reguła: pozycja jest pokazana, gdy znormalizowane zapytanie jest podciągiem znormalizowanego `text`, `section` albo nazwy jej grupy. Puste zapytanie: wszystkie `show` = `true`, `counts` = `{}`.

- [ ] **Step 1: Testy (mają nie przechodzić)**

```ts
const names = { term: "Terminal i linki", look: "Wygląd", power: "Zasilanie" };
const items = [
  { group: "term", section: "Linki", text: "Klik w link otwiera" },
  { group: "term", section: "Linki", text: "Przeglądarka poza desk" },
  { group: "term", section: "Terminal", text: "Menu tmuxa pod prawym klikiem" },
  { group: "look", section: "Motyw", text: "desk Cursor Dark" },
  { group: "power", section: "Kawa", text: "Wyłącza się sama przy przegrzaniu" },
];
it("normalize drops case, diacritics and extra spaces", () => {
  expect(normalize("  Przeglądarka   POZA desk ")).toBe("przegladarka poza desk");
  expect(normalize("Łącze żółć")).toBe("lacze zolc");
});
it("no query shows everything and counts nothing", () => {
  expect(search(items, names, "")).toEqual({ show: [true, true, true, true, true], counts: {}, query: "" });
  expect(search(items, names, "   ").query).toBe("");
});
it("matches the item text without diacritics", () => {
  const r = search(items, names, "przegladarka");
  expect(r.show).toEqual([false, true, false, false, false]);
  expect(r.counts).toEqual({ term: 1 });
});
it("a section header shows all its items", () => {
  expect(search(items, names, "linki").show).toEqual([true, true, true, false, false]); // "Terminal i linki" is the group name too
  expect(search(items, names, "motyw").show).toEqual([false, false, false, true, false]);
});
it("a group name shows the whole group", () => {
  const r = search(items, names, "zasil");
  expect(r.show).toEqual([false, false, false, false, true]);
  expect(r.counts).toEqual({ power: 1 });
});
it("nothing found", () => {
  const r = search(items, names, "xyz");
  expect(r.show.some(Boolean)).toBe(false);
  expect(r.counts).toEqual({});
});
```

- [ ] **Step 2:** `pnpm vitest run src/settings-search.test.ts` → FAIL.
- [ ] **Step 3:** Zaimplementuj. `normalize`: `toLowerCase()`, zamiana `ł` → `l`, `normalize("NFD")` z usunięciem znaków łączących (`\p{M}`), zwinięcie białych znaków, `trim()`.
- [ ] **Step 4:** `pnpm vitest run src/settings-search.test.ts` → PASS.
- [ ] **Step 5:** Commit: `Options search matches rows, section headers and group names, diacritics aside`.

---

### Task 4: Opcje w ośmiu grupach z paskiem bocznym

**Files:**
- Modify: `src/settings.ts` (`tab`, `render()`, `toggle()`)
- Modify: `src/styles.css` (blok `.settings`, ok. linii 1050 i dalej; `.dialog` tylko przez `.dialog.settings`)
- Modify: `src/locales/pl.ts`, `src/locales/en.ts`
- Modify: `src/app.ts` (klik `data-top="thermal"`, ok. linii 292)

**Interfaces:**
- Consumes: `SettingsContext.setLinkClick`, `Layout.linkClick`, `LinkClick`.
- Produces (używane w zadaniu 5):
  - `type Group = "general" | "look" | "window" | "terminal" | "projects" | "claude" | "power" | "keys"`; pole `private tab: Group = "general"`.
  - `Settings.toggle(group?: Group): Promise<void>`: otwiera na podanej grupie, bez argumentu na ostatnio oglądanej.
  - Znaczniki DOM:
    - `<button data-tab="<group>">` w `<nav class="settings-nav">`, w środku `<span class="count"></span>`;
    - `<div class="pane" data-pane="<group>">` dla każdej grupy, wszystkie w DOM, nieaktywne z `hidden`;
    - `<h2 class="pane-title">` z nazwą grupy jako pierwszy element każdego panelu;
    - każda sekcja to `<section data-section>` z opcjonalnym `<h3>` jako pierwszym dzieckiem;
    - każda pozycja do szukania ma `data-row`; sekcja będąca jednym trafieniem ma `data-row` na samym `<section>`, a „Start z aplikacją" dodatkowo `data-row="header"` (dopasowanie tylko po nagłówku).

Podział (kolejność w pasku bocznym), z kluczami nazw grup:

| Grupa (`data-tab`) | Klucz nazwy | PL / EN | Zawartość z dzisiejszego `render()` |
|---|---|---|---|
| `general` | `settings.general` | Ogólne / General | język, powiadomienia, sekcja Aktualizacje |
| `look` | `settings.look` | Wygląd / Appearance | sekcja Motyw, czcionka terminali |
| `window` | `settings.window` | Okno i układ / Window and layout | auto-układ, kolumny; sekcja „Dropdown" = cały dzisiejszy panel `dropdown` |
| `terminal` | `settings.terminal` | Terminal i linki / Terminal and links | sekcja „Linki": klik w link, przeglądarka; sekcja „Terminal": menu tmuxa |
| `projects` | `settings.projects` | Projekty / Projects | Start z aplikacją (z filtrem), Dodane foldery |
| `claude` | `settings.claude` | Claude i agenci / Claude and agents | Konta Claude, Agenci, Poprawianie promptu z VPN |
| `power` | `settings.power` | Zasilanie / Power | Kawa, Temperatura |
| `keys` | `settings.keys` | Skróty / Shortcuts | dzisiejszy panel `keys` |

Jedno trafienie (`data-row`): zwykła linia z polem; każdy skrót w `keys` (klawisz i opis w jednym elemencie, siatka dwóch kolumn zostaje dzięki `display: contents` albo `subgrid`); całe sekcje: Motyw, Konta Claude, Agenci, Dodane foldery, Poprawianie promptu, Start z aplikacją (`data-row="header"`).

Nowe teksty (oba języki):

| Klucz | PL | EN |
|---|---|---|
| `settings.linkClick` | Klik w link otwiera | A click on a link opens it |
| `settings.linkClickDesk` | w desk | in desk |
| `settings.linkClickBrowser` | w przeglądarce | in the browser |
| `settings.linkClickNone` | nic nie robi | does nothing |
| `settings.linkClickHintDesk` | ⌥-klik: przeglądarka | ⌥-click: browser |
| `settings.linkClickHintBrowser` | ⌥-klik: panel desk | ⌥-click: desk's panel |
| `settings.linkClickHintNone` | ⌘-klik: panel desk, ⌥-klik: przeglądarka | ⌘-click: desk's panel, ⌥-click: browser |
| `settings.links` | Linki | Links |
| `settings.terminalSection` | Terminal | Terminal |

Zmienione teksty: `settings.browser` → „Przeglądarka poza desk" / „Browser outside desk"; `side.empty` → „Kliknij link w terminalu albo wpisz adres powyżej." / „Click a link in a terminal or type an address above."; `settings.dropdownAbout` i `settings.dropdownKeyOff` odsyłają do grup „Skróty" i „Okno i układ" zamiast do zakładek. Klucz `settings.dropdown` zostaje jako nagłówek sekcji.

- [ ] **Step 1:** Przebuduj `render()` według tabel i znaczników. Wydziel funkcje zwracające HTML jednej grupy; `data-k` i listenery istniejących pól bez zmian. Kontrolka kliku: trzy `<button data-link-click="desk|browser|none">` w `.seg`, wybrany z klasą `sel`, obok `<span class="dim">` z podpowiedzią bieżącego trybu przez `keysIn(t(...))`; klik woła `ctx.setLinkClick` i `render()`.
- [ ] **Step 2:** CSS: `.dialog.settings` szerokość `min(860px, 92vw)`, stała wysokość `min(72vh, 620px)`; nagłówek, pod nim rząd `nav` (ok. 190 px, nie przewija się) + panel (`overflow-y: auto`), na dole stopka z `settings.where`. Zaznaczona grupa jak dzisiejsze `.tab.sel` (kolor `--accent`). Tylko zmienne motywu (`--panel`, `--line`, `--accent`…), bez kolorów na sztywno.
- [ ] **Step 3:** `toggle(group?)` w `settings.ts`; w `app.ts` klik wskaźnika temperatury woła `this.settings.toggle("power")`.
- [ ] **Step 4:** `pnpm typecheck && pnpm lint && pnpm test` → zielone (test `i18n` pilnuje zgodności kluczy pl/en).
- [ ] **Step 5: Sprawdzenie w aplikacji** (`pnpm tauri dev`), w motywie ciemnym i jednym jasnym:
  - każda z 8 grup pokazuje swoją zawartość, modal nie zmienia rozmiaru przy przełączaniu;
  - zmiana każdej przeniesionej opcji działa jak przed zmianą (język, motyw, kolumny, dropdown, nagranie skrótu, filtr projektów, dodanie i usunięcie folderu);
  - trzy tryby kliku: link WWW i ścieżka pliku w terminalu zachowują się według tabeli ze specyfikacji, w trybie „nic" zwykły klik w link zaznacza tekst i daje fokus kafelkowi;
  - wskaźnik temperatury otwiera „Zasilanie".
- [ ] **Step 6:** Commit: `Options are split into eight groups in a sidebar, with a "click on a link opens" setting`.

---

### Task 5: Szukajka w Opcjach

**Files:**
- Modify: `src/settings.ts`
- Modify: `src/styles.css`
- Modify: `src/locales/pl.ts`, `src/locales/en.ts`
- Modify: `README.md` (akapit „Podgląd obok sesji", ok. linii 60)

**Interfaces:**
- Consumes: `search`, `SearchItem` z zadania 3; znaczniki DOM i `Group` z zadania 4.

Nowe teksty: `settings.search` „Szukaj: link, czcionka, kawa…" / „Search: link, font, coffee…"; `settings.searchNone` „Nic nie pasuje. Spróbuj innego słowa albo wybierz grupę po lewej." / „Nothing matches. Try another word or pick a group on the left."

- [ ] **Step 1:** Pole `<input type="text" data-k="search">` w nagłówku modala, `private query = ""`. `render()` wstawia `value` z `this.query`, a gdy przed renderem fokus był w szukajce, przywraca go razem z pozycją kursora.
- [ ] **Step 2:** `private applySearch(): void`, wołane na końcu `render()` i na `input` szukajki (bez `render()`):
  - buduje `SearchItem[]` z elementów `[data-row]`: `group` z najbliższego `[data-pane]`, `section` z `<h3>` najbliższego `[data-section]`, `text` z `textContent` elementu (dla `data-row="header"` pusty);
  - puste zapytanie: wszystkie `[data-row]` i sekcje widoczne, widoczny tylko panel `this.tab`, liczniki puste, komunikat ukryty;
  - niepuste: `hidden` na pozycjach według `show`; sekcja ukryta, gdy nie ma w niej pokazanej pozycji; panel ukryty, gdy liczba trafień grupy to 0; `.count` w pasku bocznym z liczbą, przycisk grupy bez trafień z klasą `dim`, żadna grupa nie ma `sel`; `<p data-k="search-none">` widoczny, gdy nic nie pasuje.
- [ ] **Step 3:** Klik w grupę: `this.query = ""`, ustawia `tab`, `render()`. `onKey`: po gałęzi nagrywania skrótu, Esc przy niepustym `query` czyści je i woła `applySearch()` zamiast zamykać. `toggle()` po pierwszym `render()` stawia fokus w szukajce; `close()` zeruje `query`.
- [ ] **Step 4: Sprawdzenie w aplikacji:**
  - „przegladarka", „czcionka", „kawa", „skrot" znajdują właściwe pozycje, liczniki się zgadzają;
  - „desk" nie wypisuje listy projektów;
  - wpisywanie nie gubi fokusu; zmiana opcji w wynikach (np. tryb kliku) zostawia zapytanie, wyniki i fokus;
  - doładowanie listy projektów tuż po otwarciu nie kasuje wpisanego tekstu;
  - Esc: najpierw czyści zapytanie, potem zamyka; podczas nagrywania skrótu Esc tylko anuluje nagrywanie;
  - wklejenie konfiguracji VPN i zmiana nazwy konta działają przy aktywnym zapytaniu.
- [ ] **Step 5:** README: akapit o linkach opisuje ustawienie zamiast stałego „⌥-klik w przeglądarce" (domyślnie klik otwiera w panelu, ⌥-klik w przeglądarce; w Opcjach → Terminal i linki można to odwrócić albo wyłączyć zwykły klik).
- [ ] **Step 6:** `pnpm check` → całość zielona.
- [ ] **Step 7:** Commit: `Options have a search field that filters rows across all groups`.
