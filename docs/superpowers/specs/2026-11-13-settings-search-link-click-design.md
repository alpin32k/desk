# Opcje z szukajką i wybór, co robi klik w link

## Po co

Dwie bolączki:

1. **Klik w link.** Kliknięcie w terminal, żeby przejść do karty, trafia czasem w link i od razu
   otwiera panel albo przeglądarkę. Dziś nie da się tego wyłączyć ani odwrócić: zwykły klik zawsze
   otwiera w panelu desk, ⌥-klik zawsze w przeglądarce.
2. **Okno Opcji.** Zakładka „Ogólne" to jeden długi zwój z 12 sekcjami, „Dropdown" ma 5 pól.
   Trudno coś znaleźć, a nowa opcja nie ma oczywistego miejsca.

## Cele

- Ustawienie „Klik w link otwiera" z trzema trybami; domyślny zachowuje się dokładnie jak dziś.
- Opcje podzielone na 8 grup w pasku bocznym, z polem szukania u góry, które filtruje wiersze ze
  wszystkich grup naraz.

## Czego nie robimy

- Przycisk ↗ w panelu, pasek adresu, ⌘F i ⌥↵ zostają bez zmian.
- Żadna istniejąca opcja nie zmienia znaczenia ani klucza w `layout.json`; dochodzi jeden klucz.
- Bez nawigacji strzałkami po wynikach szukania (Tab działa jak dotąd).

## Czego nie wolno zepsuć

- Wpisywanie w szukajkę nie może zabrać fokusu ani wyzerować pola, które ktoś właśnie edytuje
  (VPN, nazwa konta, filtr projektów).
- W trybie „nic nie robi" zwykły klik w link ma działać jak klik w zwykły tekst: fokus kafelka,
  zaznaczanie, mysz programu w terminalu.
- Stary `layout.json` bez nowego klucza wczytuje się i działa jak dotąd.

## Ograniczenia z kodu

- `Settings.render()` (`src/settings.ts`) buduje cały modal przez `innerHTML` i podpina listenery
  na nowo. Wszystkie panele są w DOM (ukryte przez `hidden`), żeby każde pole miało listener.
- O celu linku decyduje jedno miejsce: `openLink` w `src/tile.ts` (`e.altKey`).
- `altClickMovesCursor: false` w opcjach xterm musi zostać (poprawka z 0.1.29).
- ⌘-klik w terminalu nie jest dziś do niczego używany.
- Teksty w `src/locales/pl.ts` i `en.ts`; kod i komentarze po angielsku.

## Podejścia

Układ modala (makiety: cztery warianty na kanwie, wybrany D):

| Wariant | Co daje | Czemu nie |
|---|---|---|
| A. Pasek boczny | krótkie grupy, miejsce na nowe opcje | wybrany jako podstawa D |
| B. Osiem zakładek u góry | najmniej pracy | ciasno już teraz, kolejna grupa się nie zmieści |
| C. Jedna strona ze spisem | wszystko widać przy przewijaniu | zwój zostaje długi |
| **D. Szukajka + pasek boczny** | A plus znajdowanie opcji po słowie, jak w palecie | **wybrany** |

Szukanie, dwa sposoby:

- osobny indeks tekstów dla każdej opcji: trzeba go utrzymywać przy każdej nowej opcji;
- **filtrowanie po tekście już wyrenderowanych wierszy** (wybrane): nic do utrzymywania, szuka
  w aktualnym języku, nie wymaga ponownego `render()`.

## Klik w link

Nowe pole `Layout.linkClick`: `"desk"` (domyślne) | `"browser"` | `"none"`; w Rust
`link_click: String` w `layout.rs`, domyślnie `"desk"`, nieznana wartość traktowana jak `"desk"`.

| Tryb | Zwykły klik | ⌥-klik | ⌘-klik (Ctrl poza macOS) |
|---|---|---|---|
| `desk` | panel desk | przeglądarka | jak zwykły klik |
| `browser` | przeglądarka | panel desk | jak zwykły klik |
| `none` | nic | przeglądarka | panel desk |

- ⌥ ma pierwszeństwo, gdy wciśnięte są oba modyfikatory.
- Dotyczy linków WWW i ścieżek plików wykrywanych w terminalu (`links.ts`). „Przeglądarka" dla
  pliku znaczy to co dziś przy ⌥-kliku: `openOutside`, czyli domyślna aplikacja systemu.
- Rozstrzyga czysta funkcja w `src/links.ts`, np.
  `linkTarget(mode, { alt, mod }): "desk" | "browser" | null`, z testami w `links.test.ts`
  (wszystkie komórki tabeli, oba modyfikatory naraz, nieznany tryb). `openLink` tylko ją woła;
  dla `null` nic nie robi.
- Linki OSC 8 (hiperłącza wysyłane przez program) nie przechodzą dziś przez `openLink`, bo w
  opcjach xterm nie ma `linkHandler`. Dostają go i idą przez tę samą funkcję, żeby każdy link
  w terminalu słuchał jednego ustawienia. (Decyzja wykonawcza, nie ustalona z użytkownikiem.)
- W trybie `none` link dalej podkreśla się pod kursorem i pokazuje „rączkę"; nieaktywny jest
  tylko sam klik bez modyfikatora. Bez śledzenia wciśniętych klawiszy. (Decyzja wykonawcza.)
- `SettingsContext.setLinkClick(mode)` obok `setBrowser`; zmiana działa od razu w otwartych
  kafelkach (tryb czytany przy kliknięciu, nie przy tworzeniu terminala).
- Teksty, które dziś mówią na sztywno „⌥-klik: przeglądarka" (`side.empty`, `settings.browser`,
  README), opisują bieżący tryb albo przestają wymieniać modyfikator.

## Okno Opcji

```
┌────────────────────────────────────────────────────┐
│ Opcje  [ Szukaj: link, czcionka, kawa…           ] │
├────────────────┬───────────────────────────────────┤
│ Ogólne         │ Terminal i linki                  │
│ Wygląd         │ LINKI                             │
│ Okno i układ   │ Klik w link otwiera               │
│▸Terminal i l.  │  [w desk][w przeglądarce][nic]    │
│ Projekty       │  ⌥-klik: przeglądarka             │
│ Claude i ag.   │ Przeglądarka poza desk [Brave ▾]  │
│ Zasilanie      │ TERMINAL                          │
│ Skróty         │ [x] Menu tmuxa pod prawym klikiem │
├────────────────┴───────────────────────────────────┤
│ Ustawienia: …/layout.json · sesje: tmux -L desk ls │
└────────────────────────────────────────────────────┘
```

Grupy (kolejność w pasku):

| Grupa | Zawartość (dzisiejsze sekcje) |
|---|---|
| Ogólne | język, powiadomienia, aktualizacje |
| Wygląd | motyw, czcionka terminali |
| Okno i układ | nowe projekty w wolne miejsce, kolumny; cała dzisiejsza zakładka „Dropdown" |
| Terminal i linki | klik w link (nowe), przeglądarka poza desk, menu tmuxa |
| Projekty | start z aplikacją (z filtrem projektów), dodane foldery |
| Claude i agenci | konta Claude, agenci, poprawianie promptu z VPN |
| Zasilanie | kawa, temperatura |
| Skróty | dzisiejsza zakładka „Skróty" bez zmian |

Zachowanie:

- Modal szerszy (ok. 860 px, nie więcej niż okno) i o stałej wysokości, żeby nie skakał przy
  zmianie grupy. Pasek boczny stoi, przewija się tylko prawa część.
- „Klik w link otwiera" to trzy przyciski obok siebie z podpowiedzią o modyfikatorach, która
  zmienia się z wyborem. Nazwy klawiszy przez istniejące pomocniki (`shortcut`, `keysIn`).
- Otwarcie Opcji stawia kursor w szukajce i pokazuje ostatnio oglądaną grupę (pierwszy raz:
  Ogólne). Otwarcie z wskaźnika temperatury w górnym pasku pokazuje „Zasilanie".
- Szukanie: bez rozróżniania wielkości liter i polskich znaków („przegladarka" znajduje
  „przeglądarka"). Dopasowanie do tekstu wiersza, nagłówka jego sekcji albo nazwy grupy. Czysta
  funkcja (normalizacja + dopasowanie) z własnymi testami.
- Co jest jednym trafieniem (decyzje wykonawcze):
  - zwykła linia z polem: jeden wiersz;
  - każdy skrót w grupie „Skróty": osobny wiersz (para klawisz + opis dostaje wspólny element);
  - motywy, konta, agenci, dodane foldery, blok poprawiania promptu z VPN: cała sekcja jako
    jedno trafienie, dopasowywana po nagłówku i widocznym tekście;
  - „Start z aplikacją": cała sekcja jako jedno trafienie, dopasowywana tylko po nagłówku.
    Nazwy projektów nie biorą udziału w szukaniu (ma je własny filtr), żeby „desk" nie zalewał
    wyników projektami.
- Trafienie w nagłówek sekcji pokazuje wszystkie jej wiersze, trafienie w nazwę grupy całą
  grupę. Licznik przy grupie to liczba pokazanych w niej trafień.
- Podczas szukania prawa część pokazuje pasujące wiersze ze wszystkich grup, pod nagłówkami grup
  i sekcji; pasek boczny pokazuje przy każdej grupie liczbę trafień, grupy bez trafień są
  przygaszone. Brak trafień: jedno zdanie zamiast pustego miejsca.
- Filtrowanie przełącza `hidden` na istniejących elementach, bez `render()`. Ponowny `render()`
  (np. po zmianie opcji) zachowuje wpisane zapytanie i jego wynik.
- Klik w grupę czyści zapytanie i pokazuje tę grupę. Esc przy niepustym zapytaniu czyści je,
  przy pustym zamyka Opcje jak dziś; nagrywanie skrótu ma pierwszeństwo jak dziś.
- Teksty odsyłające do zakładek („w zakładce Skróty", „wyłączony w zakładce Dropdown") dostają
  nowe nazwy grup.

## Zostawione wykonawcy

- Podział `render()` na mniejsze funkcje i nazwy klas CSS.
- Dokładne wymiary, odstępy i wygląd zaznaczonej grupy, w stylu reszty desk i wszystkich motywów
  (także jasnych).
- Zachowanie paska bocznego w bardzo wąskim oknie.
- Wpis w `CHANGELOG.md` i poprawka akapitu o linkach w README.
