<div align="center">

# desk

**Jedno okno. Wszystkie sesje agentów. Każdy projekt na swoim miejscu.**

Natywna aplikacja z siatką żywych terminali: po jednej sesji Claude Code
(albo Codex, opencode, Gemini, aider, shell) na projekt.

![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-000000?style=flat-square&logo=linux&logoColor=white)
![Windows](https://img.shields.io/badge/Windows-000000?style=flat-square&logo=windows&logoColor=white)
![Tauri 2](https://img.shields.io/badge/Tauri-2-000000?style=flat-square&logo=tauri&logoColor=white)
![tmux](https://img.shields.io/badge/sesje-tmux-000000?style=flat-square&logo=tmux&logoColor=white)

[Funkcje](#funkcje) · [Skróty](#skróty) · [Instalacja](#instalacja) · [Rozwój](#rozwój)

</div>

<br>

![desk: trzy projekty, dwie sesje Claude Code, testy, serwer dev i historia gita w jednym oknie](docs/screenshot.png)

<br>

<table>
<tr>
<td width="33%" valign="top">

**Sesje przeżywają okno**

Każda sesja żyje w tmux na prywatnym sockecie. Zamykasz desk, otwierasz znowu: kafelki wracają w tym samym układzie.

</td>
<td width="33%" valign="top">

**Stan widać od razu**

Hooki Claude Code mówią, kto pracuje, kto czeka, a kto jest bezczynny. Dołączane przez `claude --settings`, więc Twoja konfiguracja zostaje nietknięta.

</td>
<td width="33%" valign="top">

**Zero tokenów**

Okno nie wywołuje żadnych modeli. Limity planu czyta z plików, które agenci i tak zapisują lokalnie.

</td>
</tr>
</table>

---

## Funkcje

### Podgląd obok sesji

![Panel podglądu: plan w Markdown z diagramem mermaid, checklistą i tabelą obok sesji Claude, reszta sesji jako mini karty po lewej](docs/panel.png)

⌘W otwiera panel obok terminali. Kliknięty link (strona, `localhost`, plik) otwiera się w nim, a ⌥-klik
w przeglądarce. Klikalne są też ścieżki względne, np. `docs/plan.md`, liczone od folderu projektu kafelka.

- **Pokazuje** strony, obrazki, PDF, Markdown z diagramami mermaid, pliki `.mmd`, JSON, CSV i TSV jako
  tabelę oraz kod i tekst.
- **Edytuje** kod i tekst od razu, z kolorowaniem składni w kolorach motywu. Markdown, diagramy
  i tabele startują jako podgląd, a ✎ przełącza je na edytor. ⌘S zapisuje, ⌘Z / ⌘⇧Z cofa i ponawia.
- **Pasek adresu** działa jak w przeglądarce: wstecz, odśwież, wpisany adres albo ścieżka. „🌐 przeglądarka”
  u góry otwiera panel od razu z paskiem.
- **Strony poza desk:** ↗ otwiera stronę w Brave, a gdy go nie ma w Chrome (do zmiany w Opcjach,
  także na przeglądarkę systemową), a plik w jego domyślnym programie. Panel nie ma rozszerzeń, więc
  przy polu hasła pasek proponuje „🔑 Zaloguj w Brave (Bitwarden)”.
- **Reszta sesji** czeka po lewej jako mini karty ze stanem i ostatnimi liniami. Klik albo ⌘1–9
  przenosi sesję do środka.

### Dropdown z góry ekranu

![desk zadokowany u góry ekranu jako dropdown: trzy kolumny sesji na całą szerokość](docs/dropdown.png)

⌘` z dowolnej aplikacji: desk zjeżdża z góry ekranu, na którym jest kursor, jak okno iTerm na skrót.
Na całą szerokość, nad innymi oknami i na każdym biurku. Pasek menu zostaje od aplikacji, w której
pracujesz, a desk znika wtedy z Docka i ⌘Tab. Drugie ⌘` chowa go i wraca do poprzedniej aplikacji;
klik w inną aplikację też go chowa. Przeciągnięta dolna krawędź ustawia wysokość na stałe.

Opcje → Dropdown: wysokość, czas animacji, chowanie przy utracie fokusu i wyłączenie (przywraca zwykłe
okno). Na Windows i Linuksie skrót to Ctrl+`, włączany w Opcjach.

### Pełny ekran z zakładkami

![Sesja Claude na pełnym ekranie, nad nią zakładki wszystkich sesji z ⌘1–9](docs/fullscreen.png)

⌘E (albo ⌘↵) daje kafelkowi całe okno, a ⌘Esc wraca do siatki. Nad nim pasek zakładek: kropka stanu,
projekt, rodzaj sesji i skrót. Klik w zakładkę przełącza sesję, a zamknięcie kafelka zostawia pełny
ekran na sąsiednim.

### Skróty pod Ciebie

![Opcje, zakładka Skróty: lista akcji z ich skrótami, każdy do zmiany](docs/shortcuts.png)

Każdy skrót desk zmienisz w Opcje → Skróty: kliknij skrót i naciśnij nowy, ↺ wraca do domyślnego.
Zajęty skrót (inna akcja, ⌘C / ⌘V, ⌘Z...) desk odrzuca z komunikatem, a nowe skróty od razu widać
w dolnym pasku i podpowiedziach.

### Motywy

<table>
<tr>
<td width="50%"><img src="docs/themes.png" alt="Opcje, zakładka Ogólne: 16 motywów do wyboru"></td>
<td width="50%"><img src="docs/light.png" alt="desk w jasnym motywie GitHub Light"></td>
</tr>
</table>

16 motywów, ciemnych i jasnych: desk, Cursor Dark, One Dark, Dracula, Monokai, Nord, Tokyo Night,
Catppuccin (Mocha, Latte), Gruvbox, Rosé Pine, Solarized (Dark, Light), GitHub (Dark, Light), Ayu Light.
Zmieniają okno i terminale od razu.

### Wszystko inne

| | |
|---|---|
| **Projekty** | Sam znajduje repo git i projekty z `~/.claude/projects`. Worktree są pod repo głównym, a ręcznie dodasz dowolny folder. |
| **Nowy projekt** | ⌘N tworzy folder (opcjonalnie `git init`) i od razu otwiera w nim sesję. |
| **Autostart** | W Opcjach wybierasz projekty, które startują z desk. Domyślnie żaden. |
| **Agenci** | Claude Code, Codex, opencode, Gemini, aider, shell, a w Opcjach także własne komendy. Tab w palecie przełącza agenta. |
| **Druga sesja** | Kilka sesji w jednym projekcie (⌘D), w jednej ramce projektu. ⌘T dokłada do niej zwykły terminal. |
| **Rozmowa** | ⌘R: czat o czymkolwiek, bez projektu, każdy we własnej ramce i czystym folderze. Zachowanie ustawiasz w `rozmowa/CLAUDE.md` w danych desk. |
| **Limity** | Górny pasek: zużycie planu (5h / 7d) dla otwartych agentów, z czasem do resetu (↻2h13m). Claude z konta co 5 min i ze status line sesji, Codex z `~/.codex/sessions`. Bursztyn od 70%, czerwień od 90%. |
| **Układ** | Kolumny dobierają się do liczby projektów, sesji i rozmiaru okna. W Opcjach wyłączysz to i ustawisz liczbę kolumn ręcznie. |
| **Okno** | Otwiera się tam, gdzie je zamknięto: ten sam monitor, miejsce i rozmiar. Gdy monitora nie ma, okno otwiera się domyślnie. |
| **Kafelek** | Projekt w swoim stonowanym kolorze, branch git (klik przełącza przez `git switch`, który przy niezapisanych zmianach odmawia) i stan. Powiadomienie, gdy sesja czeka, a okno nie ma fokusu. |
| **Prawy klik** | Trafia do programu, który obsługuje mysz (vim, htop). Menu tmuxa (podział, zamiana, zamknięcie panelu) włączysz w Opcje → Ogólne. |
| **Kopiuj i wklej** | Zaznaczenie trafia do schowka (także z tmux). ⌘C / ⌘V, a na Windows i Linuksie Ctrl⇧C / Ctrl⇧V. Program zmieni schowek tylko do 3 s po Twoim kliknięciu albo klawiszu w kafelku (np. `/copy` w Claude), każda zmiana pokazuje komunikat, a czytać schowka nie może. |
| **Czcionka** | ⌘= większa, ⌘- mniejsza, ⌘0 zwykła. Rozmiar w px ustawisz też w Opcjach. |
| **Pliki** | ⌘F: drzewo plików projektu (z `git ls-files`, bez `node_modules`). Filtr po nazwie, `?tekst` szuka w treści. Klik pokazuje plik w podglądzie, dwuklik otwiera edytor. Folder ma akcje: dodaj do sesji (`@ścieżka`), terminal tutaj, pokaż w Finderze. |
| **Uruchom** | ⌘U albo ▶: skrypty z `package.json` (pnpm/yarn/npm/bun według lockfile), `cargo run`/`test`, cele `Makefile`, `docker compose up`, Django. Link `localhost` z wyjścia otwiera podgląd. |
| **Kawa** | ⌘K: komputer nie zaśnie. Drugie ⌘K: także przy zamkniętej klapce. Wyłącza się sama przy przegrzaniu albo baterii ≤15%. Działa na macOS, Windows i Linuksie. |
| **Blokada** | ⌘B: terminale widać, ale nic nie wpiszesz, dopóki nie odblokujesz: Touch ID albo hasło na macOS, Windows Hello na Windows, hasło konta na Linuksie. |

---

## Skróty

| Okno | | Kafelki | |
|---|---|---|---|
| ⌘P | projekty (paleta) | ⌘1–9 | skok do kafelka |
| ⌘N | nowy projekt | ⌘E / ⌘↵ | ekran pełny, ⌘Esc wraca |
| ⌘D | druga sesja w projekcie | ⌘J | zwiń do nagłówka z ostatnią linią (⌘J albo klik rozwija) |
| ⌘T | terminal w projekcie kafelka | ⌘Q | zamknij kafelek (sesja zostaje) |
| ⌘F | pliki projektu | ⌘⇧Q | zabij sesję |
| ⌘U | uruchom (dev serwer, testy...) | ✎ | nazwij kafelek (albo prawy klik na tytule), pusta nazwa wraca do domyślnej |
| ⌘R | rozmowa | | |
| ⌘W | podgląd obok | **Terminal** | |
| ⌘K | kawa | ⌘= / ⌘- / ⌘0 | czcionka większa / mniejsza / zwykła |
| ⌘B | blokada | ⌘C / ⌘V | kopiuj zaznaczenie / wklej |
| ⌘` | dropdown z góry ekranu | | |
| ⌘O | opcje | | |

W palecie: ↵ otwiera, ⌘↵ włącza lub wyłącza autostart, ⌘⌫ zabija sesję, Tab wybiera agenta.
Zwykłe Esc trafia do terminala, więc dalej przerywa Claude. desk zamyka się z menu (desk › Quit desk),
a ⌘⇧Q przejmuje, zanim zrobi to macOS, więc skrót nie wylogowuje z systemu.

Wszystkie poza ⌘1–9, kopiuj / wklej, klawiszami palety i ⌘S w edytorze zmienisz w Opcje → Skróty.
Na Windows i Linuksie ⌘ to Ctrl⇧ (Ctrl⇧P, Ctrl⇧D...), a zabicie sesji to Ctrl⇧⌫.

---

## Instalacja

Gotowe paczki są w [Releases](https://github.com/knehrybecki/desk/releases):

| System | Paczka |
|---|---|
| macOS (Apple Silicon, M1 i nowsze) | `.dmg` |
| Linux | `.deb`, `.AppImage` |
| Windows 10/11 | `*-setup.exe` |

Aplikacja nie jest podpisana, więc na macOS po skopiowaniu do Applications uruchom raz:

```bash
xattr -dr com.apple.quarantine /Applications/desk.app
```

### Wymagania

- macOS, Linux (webkit2gtk 4.1) albo Windows 10/11 (WebView2).
- **tmux przychodzi z desk:** jest w aplikacji na macOS i w AppImage, a paczka `.deb` instaluje go
  z repozytorium systemu. Twój własny `tmux` (3.2+) w PATH ma pierwszeństwo. Bez żadnego sesje kończą
  się razem z desk (jak na Windows), a okno o tym mówi.
- **Agentów nie instalujesz ręcznie.** Pierwszy kafelek z brakującym agentem uruchamia jego oficjalny
  instalator (Claude Code, Cursor, opencode i aider przez `curl`, Codex i Gemini przez `npm`, więc
  potrzebny jest Node.js), a potem samego agenta. W palecie taki agent ma znak ↓. desk bierze PATH
  z Twojej powłoki logowania, a do tego `~/.local/bin` i `~/.opencode/bin`.

### Aktualizacje

desk raz dziennie sprawdza, czy w Releases jest nowsza wersja (Opcje → Aktualizacje → „Sprawdź teraz”
sprawdza od razu). Repozytorium jest prywatne, więc pyta przez GitHub CLI (`gh`, zalogowane kontem
z dostępem); bez `gh` po prostu nie ma powiadomień. Nowa wersja pojawia się u góry jako `↑ 0.1.x`,
a kliknięcie pobiera ją, podmienia aplikację i uruchamia ponownie:

- macOS: podmienia `desk.app` tam, gdzie leży; sesje w tmux działają dalej,
- Windows: uruchamia nowy instalator; sesje w oknie desk się kończą,
- Linux: podmienia plik `.AppImage`; przy instalacji z `.deb` otwiera stronę wydania.

<details>
<summary><b>Windows</b></summary>

- **Sesje działają bez tmux:** trzyma je sam desk (ConPTY), więc kończą się po zamknięciu desk.
  Zamknięcie kafelka (Ctrl⇧Q) zostawia sesję, dopóki desk jest otwarty.
- **Skróty to Ctrl+Shift+litera** zamiast ⌘. Zwykłe Ctrl+C trafia do programu w terminalu,
  a kopiowanie i wklejanie to Ctrl⇧C / Ctrl⇧V. Tak samo jest na Linuksie.
- **Agenci działają w PowerShellu** (pwsh, jeśli jest zainstalowany) i po wyjściu z agenta zostaje
  PowerShell.
- **Hooki i status line uruchamiają sam `desk.exe`**, więc nie potrzeba sh. Limity Claude działają,
  a status line z `~/.claude/settings.json` wywołuje się przez Git Bash albo PowerShell, tak jak robi
  to Claude Code.
- **Kawa** trzyma komputer obudzonym. Tryb z klapką ustawia w planie zasilania „po zamknięciu
  pokrywy: nic nie rób” (`powercfg`, bez uprawnień administratora). Poprzednie ustawienie wraca po
  wyłączeniu kawy, a także wtedy, gdy desk się zamknie albo padnie.
- **Blokada** odblokowuje się przez Windows Hello (PIN, odcisk palca albo twarz). Bez Windows Hello
  ustaw PIN w Ustawienia → Konta → Opcje logowania.

</details>

<details>
<summary><b>Bezpieczeństwo i prywatność</b></summary>

- **Kawa z klapką (macOS)** używa `pmset -a disablesleep`. Za pierwszym razem macOS pyta o hasło
  administratora i desk zapisuje regułę `/etc/sudoers.d/desk-pmset`, która pozwala tylko na
  `pmset -a disablesleep 1` i `0`. Później nie pyta. Regułę usuwasz w Opcje → Kawa. Jeśli desk zginie
  w trakcie, osobny strażnik przywraca usypianie.
- **Kawa na Linuksie** używa `systemd-inhibit` (z klapką: `handle-lid-switch`), a przegrzanie
  i baterię czyta z `/sys`.
- **Blokada** chroni sesje desk, a nie cały komputer: inny program działający jako Ty nadal może
  podłączyć się do `tmux -L desk`. Cały komputer blokuj ekranem blokady systemu. Na Linuksie hasło
  konta sprawdza PAM (tak jak wygaszacze ekranu).
- **Limity Claude** desk co 5 minut pobiera sam z Anthropic (to samo zapytanie co `/usage` w Claude
  Code, bez modelu i bez tokenów), używając loginu Claude Code: na macOS z pęku kluczy
  (`Claude Code-credentials`), gdzie indziej z `~/.claude/.credentials.json`. Token idzie tylko do
  `api.anthropic.com`; desk go nie odświeża ani nie zapisuje. Do tego dochodzą odczyty ze status line
  sesji desk (świeże po każdej turze), a wygrywa najświeższy. Bez loginu zostają same sesje. Twoja
  własna status line działa dalej: desk ją wywołuje.
- **Podgląd lokalnych plików** widzi tylko folder podglądanego pliku i nie ma dostępu do API desk.
  Markdown i inne dokumenty desk renderuje sam: ich własne skrypty się nie uruchamiają (mermaid jest
  wbudowany, działa bez internetu).

</details>

---

## Rozwój

```bash
pnpm install
pnpm tauri dev          # uruchomienie w trybie deweloperskim
pnpm check              # tsc, eslint, vitest, clippy, cargo test
pnpm tauri build        # macOS: src-tauri/target/release/bundle/macos/desk.app
```

Dane: `~/Library/Application Support/desk/` (macOS) albo `~/.local/share/desk/` (Linux): `layout.json`,
`state/`, ustawienia hooków, skrypty `desk-hook.sh` / `desk-statusline.sh` i `tmux.conf`. Hooki opisuje
[docs/hooks.md](docs/hooks.md). `DESK_SUPPORT_DIR` wskazuje inny folder danych, np. dla drugiej,
testowej instancji.

Release: push tagu `v*` uruchamia `.github/workflows/release.yml`, który buduje wersje na macOS
(Apple Silicon, `.app` + `.dmg`), Linux (`.deb` + `.AppImage`) i Windows (`setup.exe`) i dołącza je
do szkicu release na GitHubie.
