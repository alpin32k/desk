# Wiatraki na maksimum przy przegrzaniu

## Po co

Przy długich buildach, testach i kilku agentach naraz procesor dochodzi do progu, a desk umie dziś
tylko jedno: zwolnić programy w swoich sesjach. Wiatraki w MacBooku Pro tymczasem stoją albo kręcą
się wolno, bo macOS rozkręca je dopiero przy wysokiej temperaturze (14.11.2026, MacBook Pro M3 Pro
bez obciążenia: oba wiatraki stoją, tryb auto, zakres 2317–6800 obr./min).

Stats umie sterować wiatrakami po doinstalowaniu helpera. Chcemy tego w desk: gdy procesor dojdzie
do ustawionej temperatury, wiatraki idą na maksimum, żeby schłodzić komputer ile się da. Co dokładnie
ma się dziać przy przegrzaniu (zwalniać, kręcić, jedno i drugie), wybiera się w Opcjach.

## Cele

- Nowa reakcja na przegrzanie: **wiatraki na maksimum** (wszystkie, pełne obroty), a po ostygnięciu
  z powrotem auto, czyli tak, jak steruje nimi macOS.
- W Opcjach (grupa Zasilanie, sekcja Temperatura) jeden wybór „Przy przegrzaniu”:
  - **Nic**: desk tylko pokazuje temperaturę.
  - **Zwalniaj sesje**: to, co dziś (domyślnie).
  - **Wiatraki, potem zwalnianie**: wiatraki od własnego, niższego progu, zwalnianie dopiero od
    progu zwalniania.
  - **Wiatraki i zwalnianie**: jeden próg, przy nim jedno i drugie.
  - **Tylko wiatraki**: bez zwalniania.
- Zgoda administratora raz, przy pierwszym wyborze trybu z wiatrakami, tak jak „Kawa” z zamkniętą
  klapką. Później desk nie pyta.
- Nie potrzeba Statsa ani niczego doinstalowywać ręcznie.

## Czego nie robimy

- Windows i Linux: tryby z wiatrakami są tam wyszarzone z dopiskiem „tylko macOS”. Sterowanie
  wiatrakami na tych systemach to osobny temat (każdy laptop inaczej).
- Intel Maki: desk wydaje tylko Apple Silicon (`release.yml`); kod nie musi obsługiwać Intela,
  ale na Intelu nie może zrobić szkody (tryby wyszarzone, zob. „Które Maki”).
- Własne krzywe, pośrednie obroty, suwak prędkości. Tylko „maksimum” i „auto”.
- Stałe pokazywanie obrotów w pasku. Obroty są tylko w podpowiedzi (title) chipa temperatury.
- Helper Statsa: przyjmuje tylko Statsa (sprawdza podpis, requirement z `identifier
  "eu.exelban.Stats"` i jego Team ID), więc desk go nie użyje.

## Czego nie wolno zepsuć (anty-cele)

- **Wiatraki nie mogą zostać w trybie ręcznym, gdy desk już nimi nie steruje**: po zamknięciu desk,
  po wysypaniu się (także `kill -9`), po wyłączeniu trybu w Opcjach, po usunięciu zgody. Ręczny tryb
  na maksimum jest bezpieczny dla sprzętu, ale głośny i zjada baterię; ręczny tryb z niskimi
  obrotami byłby groźny, dlatego helper nigdy nie ustawia nic poza „maksimum” i „auto”.
- **desk nie walczy z innym programem** (Stats, Macs Fan Control, TG Pro): gdy wiatraki są już
  w trybie ręcznym, a nie ustawił go desk, desk ich nie rusza ani na maksimum, ani z powrotem na
  auto, i mówi o tym w podpowiedzi chipa.
- **Żadnego hasła w przypadkowym momencie.** macOS pyta tylko wtedy, gdy ktoś w Opcjach wybiera tryb
  z wiatrakami (albo przy aktualizacji helpera, zob. niżej), nigdy, gdy akurat robi się gorąco.
- **Reguła sudo nie może dać roota na więcej niż dwie komendy.** Żadnych argumentów od desk do
  helpera poza `max` i `auto`, żadnego zapisu dowolnego klucza SMC, helper w miejscu, którego
  użytkownik nie może nadpisać.
- Dzisiejsze zwalnianie działa dokładnie jak dotąd w trybie „Zwalniaj sesje”, a stary `layout.json`
  wczytuje się bez zmian zachowania.

## Jak desk dostaje się do wiatraków (wybrane podejście)

Zapis do wiatraków (klucze SMC) wymaga roota. Rozważone drogi:

1. **Własny mały program `desk-fans` + reguła sudo (wybrane).** Jak „Kawa” z klapką: przy pierwszej
   zgodzie `osascript … with administrator privileges` instaluje helper i regułę sudo, która pozwala
   bez hasła na dokładnie `desk-fans max` i `desk-fans auto`. Watchdog jak przy kawie.
2. Usługa systemowa przez `SMAppService.daemon` (jak Stats): wymaga podpisu Apple Developer ID, a
   wydania desk go nie mają, więc macOS nie trzyma zgody pewnie. Odrzucone.
3. Reguła sudo na `smc` ze Statsa: leży w `/Applications`, które użytkownik może nadpisać, więc
   każdy program na koncie dostałby roota bez hasła; desk zależałby od Statsa. Odrzucone.

### Helper `desk-fans`

- Osobny, mały program w Ruście, we własnym crate `src-tauri/fans-helper/` (z pustym `[workspace]`,
  więc nie wchodzi do buildu desk), bez zależności poza IOKit i `libc`. Nie linkuje `desk_lib`, tylko
  włącza przez `#[path]` dwa pliki, których używa też desk: logikę wiatraków (`fan_control.rs`)
  i dostęp do SMC (`smc.rs`). Własny crate zamiast `src/bin/` (zmiana przy planowaniu): bundler
  Tauri pakuje każdy bin z `src/bin/` na wszystkich systemach, a helper ma trafić tylko do paczki
  macOS.
- Komendy (jedyny argument):
  - `max`: wszystkie wiatraki (`FNum`) na maksimum (`FnMx`).
  - `auto`: wszystkie wiatraki z powrotem w trybie automatycznym.
  - `version`: wypisuje numer wersji protokołu helpera (liczba całkowita, zmieniana tylko przy
    zmianie zachowania helpera). Nie wymaga roota i nie jest w regule sudo.
  - Wszystko inne: kod błędu, nic nie zapisuje.
- Apple Silicon: odblokowanie przez `Ftst` = 1, potem dla każdego wiatraka **najpierw** `FnTg` =
  `FnMx` (float), **dopiero potem** `FnMd` = 1 (ręczny). Ta kolejność sprawia, że nawet urwany
  w połowie `max` zostawia wiatrak co najwyżej w trybie ręcznym na maksimum, nigdy na starym celu
  (u nas 0, bo wiatraki stoją). Jeśli sprzęt nie przyjmie celu przed trybem ręcznym, builder
  odwraca kolejność, ale wtedy błąd po `FnMd` = 1 musi od razu kończyć się `auto`. Powrót: `FnMd`
  = 0 dla każdego, potem `Ftst` = 0. Zapis do `FnMd` po `Ftst` bywa odrzucany przez chwilę, więc
  ponawia (jak Stats: do kilku sekund, co ~50 ms). Dokładną sekwencję i typy kluczy (ui8 / flt)
  builder sprawdza w otwartym kodzie Statsa (`Kit/plugins/SMC.swift`, licencja MIT) i na tym
  Macu. Klucze są tu obecne: `Ftst`, `F0Md` (teraz 3 = auto), `F0Tg`, `F0Mx` = 6800, `FNum` = 2.
- Kod wyjścia 0 tylko wtedy, gdy po zapisie odczyt potwierdza stan (tryb ręczny i cel = maksimum,
  albo tryb auto). Gdy `max` się nie uda w połowie, helper sam robi `auto` przed wyjściem z błędem.
  Błąd: krótki komunikat na stderr, który desk pokazuje w podpowiedzi.
- Na Linuksie i Windows helper kompiluje się jako zaślepka (wypisuje „unsupported”, kod błędu).
  `fan_control.rs` jest czysty i kompiluje się wszędzie, `smc.rs` tylko na macOS, więc
  `cargo clippy --all-targets` w `check.yml` dalej przechodzi.

### Które Maki (decyzja: sprawdzenie kluczy, nie lista modeli)

- desk sam, bez roota, czyta SMC (tym samym odczytem, którego używa do obrotów w podpowiedzi):
  tryby z wiatrakami są dostępne, gdy to Apple Silicon, `FNum` > 0 i są klucze `Ftst`, `F0Md`,
  `F0Tg`, `F0Mx`. Dzięki temu działa to też na nowszych Makach (M5) bez zmiany kodu.
- Bezpieczeństwo na nieprzetestowanym sprzęcie opiera się na kolejności zapisu (cel przed trybem),
  odczycie po zapisie i `auto` po każdym błędzie, nie na liście modeli. Testy jednostkowe helpera
  (na atrapie SMC) pokrywają: urwany `max` → `auto`, odrzucony zapis → `auto`, `auto` zawsze
  próbuje wszystkich wiatraków, nawet gdy jeden się nie da.
- Tekst przy wyszarzeniu: „ten Mac nie ma wiatraków” (`FNum` = 0) albo „tylko macOS na Apple
  Silicon” (reszta).

### Instalacja (jednorazowa zgoda)

- Miejsce helpera: `/Library/PrivilegedHelperTools/pl.makarowski.desk.fans`, `root:wheel`, `0755`.
- Reguła: `/etc/sudoers.d/desk-fans`, `0440`, sprawdzana `visudo -cf` przed instalacją (jak
  `coffee::install_script`):
  `<user> ALL=(root) NOPASSWD: <helper> max, <helper> auto`
  (walidacja nazwy użytkownika jak `coffee::sudoers_rule`).
- Skrypt roota kopiuje helper z paczki desk najpierw do katalogu tymczasowego należącego do roota,
  sprawdza jego SHA-256 z sumą, którą desk policzył z tego pliku tuż przed otwarciem okna hasła
  (suma jest w treści skryptu), zdejmuje `com.apple.quarantine` (desk pobrany z GitHuba) i dopiero
  potem instaluje. Dzięki temu nic nie podmieni pliku między kliknięciem zgody a instalacją. Paczki
  podmienionej wcześniej to nie chroni, ale kto może podmienić desk.app, ten i tak przejął desk.
- Tekst okna hasła (PL/EN), np. „desk: jednorazowa zgoda na sterowanie wiatrakami przy
  przegrzaniu (tylko „maksimum” i „auto”). Później desk nie będzie już pytać.”
- Anulowane okno: wybór w Opcjach wraca do poprzedniego, toast mówi dlaczego.
- **Aktualizacja helpera:** desk porównuje `desk-fans version` zainstalowanego helpera ze swoją
  wersją protokołu. Gdy się różni, tryby z wiatrakami działają jak „bez zgody”: Opcje pokazują
  „Zaktualizuj zgodę” (znowu okno hasła), a przegrzanie tylko zwalnia (gdy tryb je obejmuje).
- **Usuń zgodę** (przycisk w Opcjach, jak przy kawie): najpierw `auto` (tylko jeśli wiatraki
  ustawił desk), potem usuwa regułę i helper (jedno okno hasła), wybór wraca do „Zwalniaj sesje”
  (gdy tryb obejmował zwalnianie) albo „Nic” („Tylko wiatraki”).

### Skąd desk bierze helper do instalacji

- W paczce: obok binarki desk (`Contents/MacOS/desk-fans`), czyli
  `current_exe().parent().join("desk-fans")`. W buildzie debug desk patrzy też do
  `src-tauri/binaries/desk-fans-aarch64-apple-darwin`.
- Jak trafia do paczki (ustalone w planie): skrypt `.github/build-fans.sh <triple>` buduje helper do
  `src-tauri/binaries/desk-fans-<triple>`, a osobny config tylko dla macOS
  (`tauri.mac-bundle.conf.json`: externalBin tmux i desk-fans) zastępuje w `release.yml` dla macOS
  `tauri.tmux.conf.json` (ten dzieli AppImage na Linuksie). Build na Linuksie i Windows się nie
  zmienia. Sumy SHA-256 nie trzeba wbudowywać w czasie budowania (zob. Instalacja).

## Decyzja: kiedy wiatraki, kiedy zwalnianie

- Dwie niezależne „polityki” na tych samych odczytach co 5 s: obecna `thermal::Policy` dla
  zwalniania i druga jej instancja dla wiatraków. Każda ma swój próg, uśrednianie 15 s, powrót 7°C
  niżej i minimum 60 s (te same stałe). Systemowe „za gorąco” (`pressure`) włącza obie.
- Progi w zależności od trybu:

  | Tryb | Wiatraki od | Zwalnianie od |
  |---|---|---|
  | Nic | – | – |
  | Zwalniaj sesje | – | próg |
  | Wiatraki, potem zwalnianie | próg wiatraków | próg |
  | Wiatraki i zwalnianie | próg | próg |
  | Tylko wiatraki | próg | – |

- Próg wiatraków (tylko w trybie „Wiatraki, potem zwalnianie”): domyślnie 70°C (5 niżej niż
  domyślny próg 75), zakres jak próg (50–100) i nie wyżej niż próg zwalniania (desk przycina).
- Gdy wiatraki nie mogą ruszyć (brak zgody, inny program steruje, błąd helpera), zwalnianie
  w trybach „potem” i „razem” działa normalnie; „Tylko wiatraki” wtedy nic nie robi poza chipem.

## Wiatraki w czasie pracy

- Wejście w „gorąco” (polityka wiatraków przechodzi na tak): desk czyta tryb wiatraków (odczyt SMC
  nie wymaga roota). Jeśli któryś jest już ręczny, a desk go nie ustawił: nic nie robi, status
  „steruje inny program”. W przeciwnym razie `sudo -n <helper> max`, zapisuje znacznik
  `fans-forced` w danych desk i odpala watchdog.
- Co odczyt, gdy wiatraki mają być na maksimum, desk sprawdza odczytem, czy dalej są w trybie
  ręcznym (np. po uśpieniu komputera SMC mógł wrócić do auto). Jeśli nie, ponawia `max`, najwyżej
  raz na minutę.
- Wyjście z „gorąca”, wyłączenie trybu w Opcjach, zamknięcie desk (`RunEvent::Exit`, obok
  `thermal::Guard::shutdown`): `sudo -n <helper> auto`, usuwa znacznik, zatrzymuje watchdog.
- **Watchdog** (jak `coffee::watchdog_script`): pętla w `sh` jako użytkownik, czeka, aż proces desk
  zniknie albo pojawi się plik stop, potem `sudo -n <helper> auto`. Działa po wysypaniu desk.
  Startuje we własnej grupie procesów, żeby Ctrl+C w `pnpm tauri dev` nie zabił go razem z desk.
- „Inny program” to też druga kopia desk (np. testowa, z innym `DESK_SUPPORT_DIR`) albo desk
  drugiego użytkownika tego Maca: każda kopia rusza tylko wiatraki, które sama ustawiła (znacznik
  w swoich danych).
- **Start desk ze znacznikiem `fans-forced`** (desk padł, a watchdog nie zdążył, np. restart w
  trakcie): `auto` i usunięcie znacznika. Bez znacznika desk przy starcie nie rusza wiatraków (to
  mógł ustawić inny program).
- Mac bez wiatraków (`FNum` = 0, MacBook Air): tryby z wiatrakami wyszarzone z dopiskiem „ten Mac
  nie ma wiatraków”.

## Wygląd

### Opcje → Zasilanie → Temperatura

Zamiast dzisiejszego checkboxa „Zwalniaj sesje, gdy procesor jest za gorący”:

```
Temperatura
Przy przegrzaniu   [Nic] [Zwalniaj sesje] [Wiatraki, potem zwalnianie] [Wiatraki i zwalnianie] [Tylko wiatraki]
Próg               [75] °C
Próg wiatraków     [70] °C          ← tylko w „Wiatraki, potem zwalnianie”
Zgoda na wiatraki jest zainstalowana (tylko „maksimum” i „auto”).  [Usuń zgodę]
   albo: Tryby z wiatrakami raz zapytają o hasło administratora.
   albo: Zgoda na wiatraki jest z poprzedniej wersji desk.  [Zaktualizuj zgodę]
<opis: co robi wybrany tryb; jak dziś `settings.thermalAbout` dla zwalniania>
```

- Przełącznik jak „Klik w link otwiera” (`seg`). Pięć opcji może się nie zmieścić w szerokości
  Opcji: wtedy builder łamie je w dwóch wierszach albo daje `<select>`; ważne, że to jeden wybór.
- Każdy wiersz ma `data-row`, żeby wyszukiwarka Opcji go znajdowała.
- Na Windows/Linuksie i na Macu bez wiatraków trzy tryby z wiatrakami są wyszarzone z `title`
  mówiącym dlaczego.

### Chip temperatury w pasku

- Wiatraki na maksimum: znak `≋` po temperaturze: `78° ≋`, razem ze zwalnianiem `81° ≋ ↓`.
- Podpowiedź (title) mówi, co się dzieje, np. „Procesor: 78°C, powyżej 70°C. Wiatraki na maksimum
  (6800 obr./min), wrócą do auto przy 63°C.” Przy problemie: „…ale desk nie steruje wiatrakami:
  <powód>” (brak zgody, inny program, błąd helpera).
- Toasty jak przy zwalnianiu: „Procesor ma 72°C: wiatraki na maksimum” i „Procesor ostygł (63°C):
  wiatraki wracają do auto”. Gdy wiatraki i zwalnianie startują w tym samym odczycie, jeden toast
  o obu.
- Klik w chip dalej otwiera Opcje → Zasilanie.

## Ograniczenia z kodu

- `src-tauri/src/thermal.rs`: `Guard::tick` liczy dziś jedną decyzję; dochodzi tryb i druga
  `Policy`. `Status` dostaje pola o wiatrakach (np. `fans: Option<FanStatus>` z `forced`, `rpm`,
  `max_rpm`, `problem`), wysyłane tym samym zdarzeniem `thermal`. Sterowanie wiatrakami najlepiej
  w osobnym module (`fans.rs`), z czystymi funkcjami do testów (reguła sudo, skrypt instalacji,
  skrypt watchdoga, decyzja na podstawie trybu).
- `src-tauri/src/coffee.rs` ma gotowe `run_admin`, `applescript_quote`, `sudoers_rule`,
  `install_script`, `watchdog_script`. Wydzielić wspólne części zamiast kopiować; `run_admin`
  potrzebuje innego tekstu okna hasła.
- `layout.rs` / `src/api.ts`: dziś `thermal_limit` (0 = wyłączone). Nowe klucze: `thermal_mode`
  (`off` | `slow` | `fansFirst` | `together` | `fans`) i `fan_limit`. Stary plik bez `thermal_mode`:
  `thermal_limit` 0 → `off` (próg domyślny 75), inny → `slow` z tym progiem. Test na stary plik.
  `save_layout` przekazuje tryb i progi do `Guard`.
- `src/thermal.ts`: `thermalHtml`, `thermalTitle`, `thermalLevel` dostają wiatraki; `src/app.ts`
  `onThermal` dostaje toasty o wiatrakach.
- `src/settings.ts`: sekcja Temperatura w grupie `power`; potrzebuje stanu zgody i dostępności
  wiatraków (nowa komenda, np. `fans_info`, albo w `thermal_status`).
- Teksty w `src/locales/pl.ts` i `en.ts`; kod i komentarze po angielsku. README (tabela „Wszystko
  inne”, wiersz Temperatura) i CHANGELOG.

## Testy

- Rust (unit): wybór progów i decyzji dla każdego trybu; dwie polityki niezależne; migracja
  starego `layout.json`; reguła sudo (dokładnie dwie komendy, odrzucona dziwna nazwa użytkownika);
  skrypt instalacji (visudo, sprawdzenie sumy, `0440`/`0755`, `root:wheel`); skrypt watchdoga;
  „inny program steruje” → desk nie rusza wiatraków; znacznik `fans-forced` przy starcie.
- TS (vitest): chip i podpowiedź z wiatrakami, z wiatrakami i zwalnianiem, z problemem; Opcje:
  pięć trybów, próg wiatraków tylko w „potem”, wyszarzenie bez wiatraków.
- Ręcznie na tym Macu (M3 Pro), bo zapisu SMC nie da się sprawdzić bez hasła: zgoda → obciążenie
  (build) → chip `≋`, obroty w Stats rosną do ~6800 → ostygnięcie → auto; zamknięcie desk przy
  wiatrakach na maksimum → auto w ciągu kilku sekund; `kill -9` desk → watchdog wraca do auto;
  usunięcie zgody → `/etc/sudoers.d/desk-fans` i helper znikają.

## Decyzje podjęte przy spisywaniu (do sprawdzenia przez właściciela)

Wybór trybu w Opcjach i droga przez własny helper z regułą sudo to decyzje właściciela. Reszta to
propozycje przy spisywaniu:

- Pięć trybów w jednym wyborze (z dzisiejszym zwalnianiem jako jednym z nich), domyślnie „Zwalniaj
  sesje”, a stary `thermal_limit` 0 → „Nic”.
- Próg wiatraków w trybie „potem” domyślnie 70°C i nie wyżej niż próg zwalniania.
- Wiatraki: tylko maksimum i auto, ten sam powrót co przy zwalnianiu (7°C niżej, min. 60 s).
- Zgoda przy wyborze trybu w Opcjach, nie przy pierwszym przegrzaniu.
- Wszystkie Apple Silicon z kluczami wiatraków, nie lista modeli.
- `≋` w chipie, toasty jak przy zwalnianiu.
- desk nie rusza wiatraków, które ustawił ktoś inny.

## Zostawione builderowi

- Dokładna sekwencja zapisu SMC i typy kluczy (sprawdzić w kodzie Statsa i na sprzęcie), w
  granicach z sekcji „Helper”.
- Czy stan zgody idzie nową komendą, czy w `thermal_status`.
- Układ przełącznika pięciu trybów, jeśli nie mieści się w jednym wierszu.
- Podział kodu między `thermal.rs`, nowy `fans.rs` i wspólne części z `coffee.rs`.
