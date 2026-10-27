# Zmiany

## Niewydane

- **Zakładki nad kafelkiem na pełnym ekranie (⌘E).** Wąski pasek pokazuje otwarte sesje w kolejności
  ⌘1–9: kropka stanu, projekt, rodzaj sesji i skrót. Bieżąca ma kolor projektu, a klik w inną
  przełącza na nią. Przy jednej sesji paska nie ma.
- **Zamknięcie sesji na pełnym ekranie nie wychodzi z niego.** Po ⌘Z (albo ⇧⌘Z, albo gdy sesja się
  skończy) ekran dostaje sąsiednia sesja, jak po zamknięciu zakładki.
- **Dropdown z góry ekranu (⌘`), jak w iTerm.** Skrót działa w każdej aplikacji: desk zjeżdża
  z animacją z góry ekranu, na którym jest kursor, na całą szerokość pod paskiem menu i nad
  innymi oknami, na każdym biurku (Space). Drugie ⌘` chowa go w górę i oddaje fokus poprzedniej
  aplikacji. Chowa się też sam po kliknięciu innej aplikacji. Pasek menu zostaje od aplikacji,
  w której pracujesz, a okno nie ma świateł (zamknij, zwiń, powiększ): w ich miejscu jest
  podpowiedź, który skrót chowa desk. Na czas trybu dropdown
  desk znika z Docka i z ⌘Tab.
- **Opcje mają trzy zakładki:** Ogólne (dotychczasowe ustawienia), Dropdown i Skróty.
- **Każdy skrót desk można zmienić** w Opcje → Skróty: kliknij skrót i naciśnij nowy, ↺ wraca do
  domyślnego. Zajęty skrót (inna akcja, ⌘C / ⌘V, ⌘Q...) desk odrzuca z komunikatem. Nowe skróty
  widać od razu w dolnym pasku i podpowiedziach. Stałe zostają ⌘1–9, kopiuj / wklej, klawisze
  palety i ⌘S w edytorze.
- W Opcje → Dropdown: wysokość, czas animacji (0 bez animacji), chowanie przy utracie fokusu
  i wyłączenie, które przywraca zwykłe okno. Skrót dropdownu zmienisz w Opcje → Skróty. Wysokość
  ustawiona przeciągnięciem dolnej krawędzi zostaje na następne razy. Na Windows i Linuksie
  domyślny skrót to Ctrl+` i jest wyłączony. W czasie blokady skrót nic nie robi.
- Esc zamyka Opcje także wtedy, gdy fokus jest w terminalu.
- Dolny pasek: skróty, które się nie mieszczą, znikają w całości, a liczniki sesji zostają.
- **Prawy klik w terminalu nie otwiera już menu.** Wyskakiwały dwa naraz: menu tmuxa (podział,
  zamiana, zamknięcie panelu) i systemowe (Wytnij, Pisownia, Mowa). Kafelki układa desk, więc
  oba zniknęły. Programy, które obsługują mysz (vim, htop), dalej dostają prawy klik. Działające
  sesje dostają nowe ustawienie przy starcie desk. Menu tmuxa można włączyć z powrotem w Opcje →
  Ogólne („Menu tmuxa pod prawym klikiem w terminalu”). Włączone zostaje otwarte po puszczeniu
  przycisku i przy ruchu myszy, a pozycję wybiera się kliknięciem.

## 0.1.12 (2026-10-27)

- **Blokada zasłania też Dock i pasek menu na drugim monitorze.** Zasłona leżała pod nimi, więc
  dało się kliknąć Dock albo pasek menu i uruchomić inną aplikację. Teraz leży nad nimi i nad
  menu, ale pod oknem Touch ID, więc to okno widać na każdym monitorze. Zasłony nie znikają też
  na czas odblokowania.

## 0.1.11 (2026-10-27)

- **Klikalne ścieżki względne i dokumenty.** Ścieżka taka jak `docs/superpowers/plans/plan.md`,
  którą agent wypisze w terminalu, otwiera się w panelu jako plik projektu kafelka. Do stron,
  obrazków i PDF doszły `.md`, `.mmd`, `.json`, `.csv`, `.tsv` i `.txt`. Sama nazwa bez folderu
  (`README.md`) zostaje zwykłym tekstem.

## 0.1.10 (2026-10-27)

- **Nazwy kafelków.** ✎ w nagłówku albo prawy klik na tytule pozwala nadać kafelkowi własną
  nazwę, np. „logi API”. Jest widoczna w nagłówku i na mini kartach, a desk ją pamięta. Pusta
  nazwa przywraca domyślną.
- **Zwijanie kafelka (⌘J albo – w nagłówku).** Zostaje sam nagłówek z ostatnią linią wyjścia, na
  przykład terminal, który tylko wypisuje logi. Sesja działa dalej. ⌘J, klik w nagłówek, ⌘1–9
  albo ekran pełny go rozwijają. Desk pamięta zwinięte kafelki.
- **Logowanie z Bitwardenem.** Przeglądarka w panelu nie obsługuje rozszerzeń, więc gdy strona
  ma pole hasła, na pasku panelu pojawia się „🔑 Zaloguj w Brave (Bitwarden)”. Jedno kliknięcie
  otwiera ją w Brave albo Chrome, gdzie działa menedżer haseł.

## 0.1.9 (2026-10-27)

- **tmux przychodzi z desk.** Na macOS i Linuksie bez tmux każda sesja kończyła się błędem
  „tmux: No such file or directory”. Teraz tmux jest w aplikacji na macOS i w AppImage, a paczka
  `.deb` instaluje go z repozytorium systemu. Własny tmux w PATH ma pierwszeństwo. Gdyby nie było
  żadnego, desk sam trzyma sesje (jak na Windows) i mówi o tym przy starcie.
- **Agenci instalują się sami.** Kafelek z agentem, którego nie ma (Claude Code, Codex, Cursor,
  opencode, Gemini, aider), uruchamia jego oficjalny instalator na widoku, a potem agenta.
  W palecie taki agent ma znak ↓.
- **Cursor** (`cursor-agent`) jest na liście agentów.

## 0.1.8 (2026-10-27)

- **Blokada zasłania wszystkie monitory.** Do tej pory przykrywała tylko ekran z oknem desk, a na
  drugim monitorze można było normalnie pracować. Teraz każdy inny ekran przykrywa ciemne okno,
  zawsze na wierzchu. Przyjmuje kliknięcia, ale nigdy nie dostaje fokusu. Monitor podłączony
  w trakcie blokady też dostaje zasłonę (w ciągu sekundy).

## 0.1.7 (2026-10-26)

- Cmd+Z i Cmd+Shift+Z znowu zamykają i zabijają sesję, gdy fokus jest w terminalu. Wyjątek
  dla pól tekstowych z 0.1.3 łapał też ukryte pole xterm. Edytor plików dalej cofa i ponawia.

## 0.1.6 (2026-10-26)

- **Kopiowanie i wklejanie w terminalach** przez systemowy schowek: Cmd+C / Cmd+V
  (Ctrl+Shift+C / V na Windows i Linux) oraz zaznaczenie myszą w tmux (OSC 52).
- **Rozmiar czcionki terminala**: Cmd+= / Cmd+- / Cmd+0 (Ctrl+Shift na Windows i Linux) i w Opcjach,
  zapisywany w układzie.
- Programy w terminalu mogą ustawić schowek tylko do 3 s po Twoim kliknięciu lub klawiszu
  w tym kafelku, do 1 MB, zawsze z powiadomieniem. Zablokowane próby też są pokazane. Liczą się
  tylko prawdziwe kliknięcia i klawisze, nie odpowiedzi, które terminal wysyła sam. Programy nie
  mogą czytać schowka.

## 0.1.5 (2026-10-26)

- Limity Claude są pobierane z konta co 5 minut (to samo, co `/usage`, bez modelu), więc
  bezczynny desk nie pokazuje już starych liczb. Token idzie do curl przez stdin i nigdzie nie
  jest zapisywany.
- Z kilku sesji wygrywa najświeższy odczyt limitów, a nie najnowszy plik. Okno 5h nie znika.

## 0.1.4 (2026-10-25)

- Ścieżki plików łamane na kilka wierszy terminala są klikalne w całości.

## 0.1.3 (2026-10-25)

- **Edytor w panelu z kolorowaniem składni** (CodeMirror, kolory z motywu). Pliki kodu otwierają
  się od razu do edycji. Markdown, diagramy i tabele startują jako podgląd.
- Rozmowy z Cmd+R mają własną ramkę, jak nowy projekt.
- Aktualizacja pobiera paczkę do nowego, prywatnego folderu i usuwa ją po instalacji.

## 0.1.2 (2026-10-25)

- **Powiadomienie o aktualizacji i instalacja** z górnego paska (sprawdzane raz dziennie lub
  z Opcji).
- Przycisk przeglądarki w górnym pasku. Strony WWW otwierają się w Brave, a bez niego w Chrome.
- **Drzewo plików projektu w panelu (⌘F)**: filtr nazw, `?tekst` szuka w treści plików,
  „dodaj do sesji” wkleja `@ścieżka` do aktywnego Claude.
- Zmienne ze skryptów pnpm/npm nie trafiają już do sesji (pnpm odmawiał wersji projektu).

## 0.1.1 (2026-10-25)

- **Blokada i kawa na Windows i Linux**: Windows Hello (PIN, odcisk palca, twarz) i hasło konta
  przez PAM. Na Windows kawa przy zamkniętej klapie i ochrona baterii.
- **16 motywów**, ciemnych i jasnych, w Opcjach.
- **Panel boczny (⌘W)**: podgląd linków i plików, Markdown, Mermaid, przeglądarka. Edycja plików
  tekstowych (✎, ⌘S), a ↗ otwiera plik w domyślnej aplikacji. Przy otwartym panelu aktywna sesja
  jest na środku, a reszta to mini karty z lewej.
- Kolumny dopasowują się do liczby projektów, sesji i rozmiaru okna.
- desk uruchomiony z terminala w Claude Code nie przekazuje sesjom zmiennych sesji nadrzędnej.
- Sesje Claude z folderu tymczasowego systemu nie pojawiają się na liście projektów.

## 0.1.0 (2026-10-25)

Pierwsze wydanie: siatka terminali w tmux, po jednej sesji agenta na projekt, stan z hooków
Claude Code, wykrywanie projektów, kolory projektów, kawa, blokada (Cmd+B) z Touch ID, limity planu
w górnym pasku, ramki projektów z kilkoma sesjami (⌘D, ⌘T), nowy projekt (⌘N), rozmowa (⌘R),
przełączanie brancha, uruchamianie komend projektu (⌘U), sesje natywne na Windows (ConPTY).
