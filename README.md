# desk

Natywna aplikacja (Tauri 2, macOS, Linux i Windows): jedno okno z siatką żywych terminali,
po jednej sesji Claude Code (albo Codex, opencode, Gemini, aider, shell) na projekt.

![desk](docs/screenshot.png)

- **Sesje żyją w tmux** na prywatnym sockecie (`tmux -L desk`, własny `tmux.conf`), więc
  zamknięcie desk ich nie zabija. Po ponownym otwarciu kafelki wracają w tym samym układzie.
- **Stan z hooków Claude Code** (pracuje / czeka / bezczynna / shell). Hooki są doklejane przez
  `claude --settings`, więc globalna konfiguracja Claude Code i tmux zostaje nietknięta.
- **Okno nie wywołuje żadnych modeli** i nie zużywa tokenów. Limity planu czyta z plików, które
  agenci i tak zapisują lokalnie.

## Funkcje

| | |
|---|---|
| Projekty | Sam znajduje repo git i projekty z `~/.claude/projects`. Worktree są pokazane pod repo głównym, a ręcznie można dodać dowolny folder. |
| Nowy projekt | ⌘N: tworzy folder (opcjonalnie `git init`) i od razu otwiera w nim sesję. |
| Autostart | W opcjach wybierasz, które projekty startują z desk. Domyślnie żaden. |
| Agenci | Claude Code, Codex, opencode, Gemini, aider, shell, a w opcjach także własne komendy. Tab w palecie przełącza agenta. |
| Druga sesja | Kilka sesji w jednym projekcie (⌘D), w jednej ramce projektu. ⌘T dokłada do niej zwykły terminal. |
| Rozmowa | Zwykły czat o czymkolwiek, niezwiązany z żadnym projektem (⌘R). Każde ⌘R otwiera nową rozmowę w ramce aktywnego projektu, jak ⌘D, ale działa ona w swoim czystym folderze. Jej zachowanie ustawiasz w `rozmowa/CLAUDE.md` w danych desk. |
| Limity | W górnym pasku zużycie planu (5h / 7d) dla otwartych agentów: Claude z wejścia status line, Codex z `~/.codex/sessions`. Obok każdego okna czas do resetu (↻2h13m). Kolor bursztynowy od 70%, czerwony od 90%. |
| Kafelek | Projekt (każdy ma swój stonowany kolor), branch git (kliknij, aby przełączyć: `git switch`, który przy niezapisanych zmianach odmawia), stan. Powiadomienie, gdy sesja czeka, a okno nie ma fokusu. |
| Pliki | ⌘F: wyszukiwarka plików projektu (z `git ls-files`, bez `node_modules`). ↵ otwiera w edytorze (VS Code, Cursor, Zed albo Sublime, a bez nich w domyślnym edytorze tekstu), ⌥↵ w podglądzie desk. |
| Uruchom | ⌘U albo ▶: skrypty z `package.json` (z pnpm/yarn/npm/bun według lockfile), `cargo run`/`test`, cele z `Makefile`, `docker compose up`, Django. Uruchamia się w ramce projektu, a link `localhost` z wyjścia kliknięciem otwiera podgląd. |
| Podgląd | Kliknięcie linku (http, lokalny html/svg/png/pdf) otwiera szybki podgląd w oknie desk, a ⌥-klik otwiera go w przeglądarce. |
| Kawa | ⌘K: komputer nie zaśnie. Drugie ⌘K: także przy zamkniętej klapce. Wyłącza się sama przy przegrzaniu lub baterii ≤15%. |
| Blokada | ⌘B: ekran zostaje włączony, terminale widać, ale nic nie da się wpisać, dopóki nie odblokujesz Touch ID albo hasłem (macOS). |

## Skróty

| Skrót | Akcja |
|---|---|
| ⌘P | projekty (paleta) |
| ⌘N | nowy projekt (folder + opcjonalnie git init) |
| ⌘D | druga sesja w tym projekcie |
| ⌘T | terminal (zwykły shell) w projekcie aktywnego kafelka |
| ⌘F | pliki projektu |
| ⌘U | uruchom (dev serwer, testy...) |
| ⌘R | rozmowa (czat bez projektu) |
| ⌘Z | zamknij kafelek (sesja w tmux zostaje) |
| ⌘⇧Z | zabij sesję |
| ⌘1–9 | skok do kafelka |
| ⌘E / ⌘↵ | ekran pełny kafelka, ⌘Esc wraca |
| ⌘K | kawa |
| ⌘B | blokada |
| ⌘O | opcje |

W palecie: ↵ otwiera, ⌘↵ włącza lub wyłącza autostart, ⌘⌫ zabija sesję, Tab wybiera agenta.
Zwykłe Esc trafia do terminala, więc dalej przerywa Claude.

## Windows

- **Sesje działają bez tmux:** trzyma je sam desk (ConPTY), więc kończą się po zamknięciu desk.
  Zamknięcie kafelka (Ctrl⇧Z) zostawia sesję, dopóki desk jest otwarty.
- **Skróty to Ctrl+Shift+litera** zamiast ⌘ (Ctrl⇧P projekty, Ctrl⇧D druga sesja...). Zabicie
  sesji to Ctrl⇧⌫. Zwykłe Ctrl+C trafia do programu w terminalu, a kopiowanie i wklejanie to
  Ctrl⇧C / Ctrl⇧V. Tak samo jest na Linuksie.
- **Agenci działają w PowerShellu** (pwsh, jeśli jest zainstalowany) i po wyjściu z agenta
  zostaje PowerShell.
- **Hooki i status line uruchamiają sam `desk.exe`**, więc nie potrzeba sh. Limity Claude
  działają, a status line z `~/.claude/settings.json` wywołuje się przez Git Bash albo PowerShell,
  tak jak robi to Claude Code.
- **Kawa** trzyma komputer obudzonym. Zamknięcie klapki ustawisz w Opcjach zasilania Windows.
  Blokada (Touch ID) jest tylko na macOS.

## Wymagania

- macOS, Linux (webkit2gtk 4.1) albo Windows 10/11 (WebView2)
- macOS i Linux: `tmux` (3.2+) w PATH, np. `brew install tmux`
- agenci, których chcesz używać (`claude`, `codex`, ...). desk bierze PATH z Twojej powłoki
  logowania, więc widzi to samo co terminal.

## Uwagi

- **Kawa z klapką (macOS)** używa `pmset -a disablesleep`. Za pierwszym razem macOS pyta o hasło
  administratora i desk zapisuje regułę `/etc/sudoers.d/desk-pmset`, która pozwala tylko na
  `pmset -a disablesleep 1` i `0`. Później nie pyta. Regułę usuwasz w Opcje → Kawa. Jeśli desk
  zginie w trakcie, osobny strażnik przywraca usypianie.
- **Blokada** chroni sesje desk, a nie cały komputer: inny program działający jako Ty nadal może
  podłączyć się do `tmux -L desk`. Do zablokowania całego Maca używaj ekranu blokady macOS.
  Na Linuksie blokada jest niedostępna.
- **Limity Claude** pojawiają się po pierwszej odpowiedzi w sesji uruchomionej przez desk, bo
  Claude Code przekazuje je do status line dopiero wtedy. Twoja własna status line działa dalej:
  desk ją wywołuje.
- **Podgląd lokalnych plików** widzi tylko folder podglądanego pliku. Okno podglądu nie ma
  dostępu do API desk.

## Instalacja

Gotowe paczki są w [Releases](https://github.com/knehrybecki/desk/releases): `.dmg` na macOS
(tylko Apple Silicon, M1 i nowsze), `.deb` i `.AppImage` na Linux, `*-setup.exe` na Windows. Aplikacja nie jest
podpisana, więc na macOS po skopiowaniu do Applications uruchom raz:

```bash
xattr -dr com.apple.quarantine /Applications/desk.app
```

## Rozwój

```bash
pnpm install
pnpm tauri dev          # uruchomienie w trybie deweloperskim
pnpm check              # tsc, eslint, vitest, clippy, cargo test
pnpm tauri build        # macOS: src-tauri/target/release/bundle/macos/desk.app
```

Dane: `~/Library/Application Support/desk/` (macOS) albo `~/.local/share/desk/` (Linux). Są tam
`layout.json`, `state/`, ustawienia hooków, skrypty `desk-hook.sh` / `desk-statusline.sh` i `tmux.conf`.
Szczegóły hooków są w [docs/hooks.md](docs/hooks.md).

Release: push tagu `v*` uruchamia `.github/workflows/release.yml`, który buduje wersje na macOS
(Apple Silicon, `.app` + `.dmg`), Linux (`.deb` + `.AppImage`) i Windows (`setup.exe`) i dołącza je do szkicu
release na GitHubie.
