# Szczegóły limitów po kliknięciu w licznik

## Po co

Licznik w górnym pasku (`claude 5h 50% ↻2h56m · 7d 52% ↻6d3h`) mówi, ile limitu zużyto, ale nie
mówi, co z tego wynika:

- czy przy obecnym tempie limit wystarczy do resetu, a jeśli nie, o której się skończy;
- że Fable ma własny tygodniowy limit (14.11.2026: 95%, `critical`), którego licznik w ogóle nie
  pokazuje;
- czy jest reset do użycia;
- które sesje zjadają limit i która za chwilę zje więcej, bo ma zimny cache;
- jak zużycie rozkładało się w czasie.

Klik w licznik otwiera dymek z odpowiedziami na te pytania. Makiety (pięć wariantów, wybrane 4 i 5)
są poza repozytorium; to, co z nich wynika dla wyglądu, opisuje sekcja „Wygląd” i ten tekst
wystarcza do budowy.

## Cele

- Klik w licznik (każdy: konto Claude, Codex) otwiera dymek pod nim.
- Dwa układy dymka do wyboru w Opcjach: **zakładki** (wariant 5, domyślny) i **wszystko naraz**
  (wariant 4).
- Dwa sposoby liczenia udziału sesji w oknie 5 h do wyboru w Opcjach: **z kosztu sesji desk**
  (domyślny) i **z transkryptów Claude Code**.
- Nowa grupa w Opcjach: **Statystyki AI**, z tymi dwoma ustawieniami.
- desk zaczyna zapisywać historię limitów (i koszty sesji), żeby był wykres i podział dzienny.

## Czego nie robimy

- desk nie używa resetów ani nie włącza kredytów (żadnego `POST`; tylko pokazuje i podpowiada
  `/limit-reset` w Claude Code).
- Żadnych nowych zapytań do Anthropic: wszystko z tej samej odpowiedzi `/api/oauth/usage`, którą
  desk już pobiera co 5 min na konto. Zero tokenów, jak dotąd.
- Bez powiadomień o limitach (może później).
- Bez sesji i udziału dla Codexa: jego dymek ma tylko „Teraz” i „Historię”.
- Bez eksportu historii i bez historii dłuższej niż 8 dni.
- Tekst samego licznika w pasku się nie zmienia (zmienia się tylko jego kolor i dochodzi kropka,
  zob. „Licznik w pasku”).

## Czego nie wolno zepsuć

- Odświeżanie paska co 30 s i przy zdarzeniach hooków nie może zamykać ani mrugać otwartym
  dymkiem.
- Token logowania nigdy w logach, w plikach desk ani na liście procesów (jak dziś: nagłówki przez
  stdin `curl`).
- Transkrypty Claude Code tylko do odczytu.
- Czytanie transkryptów (tryb A) nie może przycinać okna: poza wątkiem UI, przyrostowo.
- Stary `layout.json` bez nowych kluczy wczytuje się i działa jak dotąd.
- Komenda `usage` dalej zwraca okna 5 h i tydzień jak dziś; nowe dane (poziom z API, limity
  modeli) dochodzą jako dodatkowe pola.

## Ograniczenia z kodu

- `renderChrome()` (`src/app.ts`) przebudowuje cały `.top` przez `innerHTML` co odświeżenie, więc
  dymek musi żyć poza `.top` (osobny element w `#app` albo `body`) i po każdym `renderChrome`
  tylko odświeżyć treść i pozycję pod licznikiem, odnajdując go po atrybucie.
- Licznik to dziś `<span class="usage">` z `title` (`usageHtml` w `src/usage.ts`); musi stać się
  `<button>` z atrybutem wskazującym agenta i konto, np. `data-usage="claude:work"`.
- Limity: `src-tauri/src/usage.rs`. `account()` trzyma w pamięci całą odpowiedź Anthropic
  (`Option<Value>`) i wyciąga z niej tylko `five_hour`/`seven_day`. Resztę pól wystarczy z niej
  przeczytać.
- Pliki `state/<slug>.status` (wejście status line'u Claude Code, zapisuje je `STATUSLINE_SCRIPT`)
  mają per sesja: `model`, `effort.level`, `fast_mode`, `cost.total_cost_usd` (od startu sesji,
  po cenach API), `context_window.used_percentage`, `prompt_cache.warm`, `prompt_cache.expires_at`,
  `prompt_cache.recache_tokens_if_cold`, `session_id`, `transcript_path`, `rate_limits`. Nazwa
  pliku to slug kafelka. Stan sesji (pracuje/czeka/wolna) to hooki (`this.hook`).
- Konto sesji: `accounts::owner_of` po ścieżce transkryptu (jak w `claude()`).
- Opcje: grupy w `src/settings.ts` (`Group`, `GROUP_NAMES`), przełączniki „seg” jak „Klik w link
  otwiera”. Klucze w `Layout` (`src/api.ts`) i `layout.rs` (domyślne w `Default`, test na stary
  plik). Teksty w `src/locales/pl.ts` i `en.ts`; kod i komentarze po angielsku.

## Co mówi Anthropic (odpowiedź z 14.11.2026, ten komputer)

`GET /api/oauth/usage` zwraca poza `five_hour` i `seven_day` m.in.:

```json
"limits": [
  {"kind": "session", "group": "session", "percent": 61, "severity": "normal",
   "resets_at": "2026-11-14T03:49:59.598735+00:00", "scope": null, "is_active": false},
  {"kind": "weekly_all", "group": "weekly", "percent": 54, "severity": "normal", "...": "..."},
  {"kind": "weekly_scoped", "group": "weekly", "percent": 95, "severity": "critical",
   "resets_at": "2026-11-20T03:59:59.598916+00:00",
   "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}, "is_active": true}
],
"weekly_scoped_shares": [
  {"limit_index": 2, "used_percent_of_weekly": 48, "allowance_percent_of_weekly": 50}
],
"extra_usage": {"is_enabled": false, "...": "..."},
"spend": {"enabled": false, "can_purchase_credits": false, "...": "..."},
"juniper_tide": null,
"cedar_ember": null
```

- `limits[]` z `kind: "weekly_scoped"` to limity modeli (dziś Fable). `weekly_scoped_shares`
  mówi, ile tygodnia zjadł ten model i ile najwyżej może (Fable: 48 z dozwolonych 50 punktów
  tygodnia).
- `extra_usage.is_enabled` / `spend.enabled`: czy po limicie idą kredyty.
- Resety (`juniper_tide`, `cedar_ember`) są dziś `null`. Ich kształt znamy tylko ze schematu
  w Claude Code 2.1.325 (komenda `/limit-reset`):
  - `juniper_tide`: reset okna 5 h raz w tygodniu, liczony do limitu tygodnia:
    `{eligible, available, next_available_at}`;
  - `cedar_ember`: zapisane resety: `{eligible, grants: [{id, label, resets_total, resets_left,
    starts_at, ends_at, clears, paused, usable_now, use_requires_limit, gives_back_weekly, type,
    unavailable_reason}], next_grant_id}`. Reset limitu Fable oddaje tylko to, co Fable zużył
    w tym tygodniu.
- Status line Claude Code może też nieść `rate_limits.model_scoped[]` (`display_name`,
  `utilization`, `resets_at`); w obecnych plikach `.status` tego nie ma.

## Podejścia

Układ dymka (pięć makiet na kanwie):

| Wariant | Co daje | Decyzja |
|---|---|---|
| 1. Tempo i prognoza | limity i prognoza | wchodzi jako zakładka/kolumna „Teraz” |
| 2. Sesje | tabela sesji | wchodzi jako „Sesje” |
| 3. Historia | wykres i dni | wchodzi jako „Historia” |
| **4. Wszystko naraz** | wszystko widać od razu; ok. 900 px szerokości | **opcja** |
| **5. Zakładki** | mieści się w trybie dropdown (580 px), kropka ostrzeżenia na zakładce | **opcja, domyślna** |

Udział sesji w oknie 5 h:

| Sposób | Plusy | Minusy | Decyzja |
|---|---|---|---|
| **A. Z transkryptów** | wszystkie sesje Claude Code na komputerze, też poza desk | tabela cen do aktualizacji, czytanie dużych plików | **opcja** |
| **B. Z kosztu w `.status`** | koszt liczy Claude Code (z modelem i fast mode), lekkie, bez cennika | tylko sesje desk; pierwsze okno po starcie desk niepełne | **opcja, domyślna** (domyślność nie ustalona z użytkownikiem) |

## Licznik w pasku

- Tekst bez zmian (`claude 5h 61% ↻1h54m · 7d 54% ↻5d3h`). Licznik staje się przyciskiem.
- Kolor (ramka i tekst, jak dziś klasy `warn`/`high`) z najgorszego ze wszystkich ostrzeżeń
  konta, nie tylko z 5 h i tygodnia:
  - okna 5 h i tydzień: `severity` z API, bez niej progi 70/90% (`level()`);
  - limity modeli (Fable): to samo;
  - prognoza: dochodzi do 100% przed resetem = co najmniej `warn`.
- Kropka po tekście (w kolorze poziomu), gdy ten poziom bierze się z czegoś, czego tekst nie
  pokazuje: z limitu modelu albo z prognozy. Przykład z 14.11.2026: 5 h 61%, tydzień 54%, Fable
  95% `critical` → licznik czerwony z kropką.
- Podpowiedź (`title`) wymienia powód, np. „Fable 95% (krytyczny)”, „tydzień: przy tym tempie
  koniec dziś ok. 21:00”.
- Ta sama reguła zapala czerwoną kropkę na zakładce „Teraz”.
- Dane: `Usage` z komendy `usage` dostaje `severity` przy oknach i listę limitów modeli
  (`name`, `usedPercent`, `resetsAt`, `severity`); prognozę liczy frontend.

## Dymek

### Wspólne

- Klik w licznik otwiera dymek pod nim, wyrównany do prawej krawędzi licznika, z „dzióbkiem”.
  Drugi klik, Esc albo klik poza dymkiem zamyka. Schowanie desk (dropdown) też zamyka.
- Otwarty dymek odświeża treść przy każdym odświeżeniu limitów, bez zamykania i bez zmiany
  zakładki ani przewinięcia.
- Nagłówek: „Limity planu · claude” (z nazwą i kolorem konta, gdy kont jest kilka), godzina
  odczytu. Stopka: „z sesji co 30 s · z Anthropic co 5 min”.
- Wysokość najwyżej do dolnej krawędzi okna; dalej przewija się wnętrze.
- Kolory: paleta desk (`--accent`, `--waiting`, `--danger`, `--working`); Fable w osobnym kolorze
  (fioletowy z makiet, do dodania jako zmienna motywu, żeby działał w motywach jasnych).

### Zakładki (domyślny)

- Szerokość ok. 580 px. Zakładki: **Teraz**, **Sesje** (licznik sesji), **Historia**.
- Na „Teraz” kropka w kolorze najgorszego ostrzeżenia, według reguły z „Licznik w pasku”.
- desk pamięta ostatnią zakładkę do zamknięcia aplikacji (w pamięci, nie w `layout.json`).
  (Decyzja wykonawcza.)
- Codex: tylko „Teraz” i „Historia”.

### Wszystko naraz

- Ok. 900 px: lewa kolumna „Teraz”, prawa „Historia”, pod spodem „Sesje” na całą szerokość
  (z kolumną „Kontekst”).
- Gdy okno jest węższe niż dymek + marginesy: jedna kolumna (Teraz, Historia, Sesje), przewijana.

### Teraz

- Dla każdego okna (5 h, tydzień): pasek z %, znacznik „ile czasu okna minęło”, godzina resetu
  i odliczanie.
- Prognoza ze średniego tempa od początku okna (zużycie / minęły czas; początek okna =
  `resets_at` − długość okna):
  - dojdzie do 100% przed resetem: ostrzeżenie „tempo X%/h → 100% ok. HH:MM, N przed resetem”
    (dla tygodnia z dniem);
  - nie dojdzie: spokojna linia z prognozą na koniec okna;
  - tydzień: „żeby dociągnąć do resetu: najwyżej X% dziennie” (pozostałe % / pozostałe dni);
  - za wcześnie na prognozę (minęło < 10% okna): bez prognozy.
- Limity modeli (z `limits[]` `weekly_scoped`, a gdy brak odpowiedzi API, z `model_scoped`
  w `.status`): pasek, %, reset, udział w tygodniu z `weekly_scoped_shares` („zużył 48% tygodnia,
  może najwyżej 50%”) i co po limicie: „kredyty” albo „kredyty wyłączone: zostaje inny model”.
  Bez limitów modeli sekcji nie ma.
- `severity` z API (`normal`/`warning`/`critical`) ma pierwszeństwo przed progami 70/90%
  (`level()` w `src/usage.ts`).
- Resety: sekcja tylko, gdy `juniper_tide` albo `cedar_ember` są i `eligible`.
  - Reset okna 5 h: „dostępny” albo „użyty, znów od …”.
  - Zapisane: każdy jako „etykieta · zostało X z Y · do dd.mm”, dopisek „działa dopiero przy
    limicie” (gdy `use_requires_limit`) i „w Claude Code: /limit-reset”.
  - Gdy prognoza 5 h dochodzi do 100% i reset jest dostępny: linia „ok. HH:MM skończy się okno
    5 h: wtedy przyda się reset”.

### Sesje (tylko Claude)

- Sesje desk tego konta, które mają plik `.status`. Kolumny: sesja (nazwa jak w kafelku), stan
  (hooki), model · effort (⚡ przy fast mode), **udział w oknie 5 h** (A albo B, z dopiskiem
  „szacunek”), koszt od startu sesji („cennik API, nie rachunek”), cache (odliczanie do
  `expires_at` albo „zimny”), w układzie „wszystko naraz” też kontekst %.
- Kolejność: udział malejąco, potem koszt.
- Wiersz „reszta” z udziałem sesji już zamkniętych (B) albo spoza desk (A, z nazwą projektu
  z folderu transkryptu, gdy się mieści).
- Ostrzeżenie o zimnym cache dla sesji z największym `recache_tokens_if_cold` (gdy > 50k):
  „następna wiadomość w X wyśle od nowa ~N tokenów”.
- Klik w wiersz: fokus na kafelku tej sesji i zamknięcie dymka.
- Stopka: suma kosztów i którym sposobem liczony jest udział.

### Historia

- Okno 5 h, ostatnie 24 h: słupki co 30 min (najwyższa wartość w przedziale), przerywane linie
  w miejscach resetu z godziną, słupki ≥ 100% czerwone, pod spodem „100% od HH:MM do HH:MM”, gdy
  limit był wyczerpany.
- Tydzień dzień po dniu: przyrost w każdym dniu bieżącego okna tygodniowego (od `resets_at` − 7 dni),
  linia „równo: 14% dziennie”, dni powyżej równego podziału na czerwono.
- Gdy jest limit modelu (Fable): w „Tydzień dzień po dniu” drugi, cieńszy słupek każdego dnia
  w kolorze modelu, z jego przyrostem. Przy kilku limitach modeli każdy ma swój słupek.
  (Decyzja wykonawcza, nie ustalona z użytkownikiem.)
- Pusta historia: „desk zbiera historię od HH:MM; wykres zapełni się w ciągu doby”.

## Wygląd

- Ciemne tło dymka (`--panel`), ramka `--line-hi`, zaokrąglenie 8 px, cień; czcionka terminala
  desk (`--mono`), 11–12 px; nagłówki sekcji wersalikami, 10,5 px, szare, z odstępem liter.
- Pasek limitu: 8 px wysokości, zaokrąglony, tło `--line`, wypełnienie w kolorze poziomu
  (`--accent`, `--waiting`, `--danger`). Znacznik minionego czasu okna: pionowa kreska 2 px
  w kolorze tekstu, wystająca 4 px nad i pod pasek; pod paskiem „▲ minęło N% okna” wyrównane
  do kreski. Pasek dłuższy niż kreska = zużycie szybsze niż upływ czasu. Obok paska % dużą,
  pogrubioną cyfrą.
- Ostrzeżenia: ikona trójkąta (obrys) + tekst. `--waiting`: prognoza dochodzi do 100% przed
  resetem. `--danger`: limit `critical` lub ≥ 90%, albo prognoza kończy limit ponad dobę przed
  resetem.
- Limit modelu (Fable): osobny kolor, fioletowy (#d2a8ff w motywie desk), jako nowa zmienna
  motywu z odpowiednikiem w motywach jasnych. Ten sam kolor w nazwie modelu w tabeli sesji,
  w udziale sesji na tym modelu i w słupkach modelu w historii.
- Sekcje, których dane są niepewne albo opcjonalne (limit modelu, resety), w ramce
  przerywanej.
- Zakładki: tekst, pod aktywną kreska 2 px `--accent`; kropka ostrzeżenia `--danger` 7 px;
  licznik sesji w szarej pigułce.
- Tabela sesji: kropka stanu w kolorach hooków (`--working`, `--waiting`, `--idle`); wiersz pod
  kursorem podświetlony z dopiskiem „→ kafelek”; udział jako mini pasek + %; „zimny” cache
  w `--waiting`; wiersz „reszta” szary.
- Historia: słupki w kolorze poziomu (< 70% `--accent`, 70–99% `--waiting`, 100% `--danger`),
  przerywane linie resetów w `--line-hi` z godziną pod osią; w dniach przerywana pionowa linia
  równego podziału.

## Zapis historii

- Nowy moduł Rust (np. `usage_history.rs`), wołany przy każdej komendzie `usage` (co 30 s).
- Plik `<support_dir>/usage-history.jsonl`, linia na zmianę:
  - limit: `{"t":…,"agent":"claude","account":"work|null","label":"5h|7d|Fable","pct":61,"resets":…}`,
    zapis tylko gdy `pct` albo `resets` się zmieniły od ostatniej linii tego klucza;
  - koszt sesji (dla B): `{"t":…,"session":"<session_id>","slug":"…","account":…,"cost":12.34}`,
    zapis tylko gdy koszt się zmienił.
- 8 dni: przy starcie i raz na dobę plik jest przepisywany bez starszych linii.
- Uszkodzona linia jest pomijana.

## Udział w oknie 5 h

Okno: od `resets_at` (5 h) − 5 h do teraz.

Udział to część tego, co zużyto w tym oknie: wiersze sesji razem z „resztą” sumują się do 100%.
To nie są punkty limitu 5 h (w trybie B ruch spoza desk jest niewidoczny, więc punkty
zawyżałyby sesje desk).

**B. Z kosztu sesji desk** (domyślny)

- Udział sesji = przyrost jej `cost` w oknie / suma przyrostów wszystkich sesji konta.
- Koszt na początku okna = ostatni zapis kosztu tej sesji sprzed początku okna; brak zapisu:
  0, gdy desk zapisywał już historię przed początkiem okna (sesja zaczęła się w oknie), inaczej
  pierwszy zapis w oknie i dopisek „niepełne”.

**A. Z transkryptów Claude Code**

- Pliki `<config_dir konta>/projects/**/*.jsonl` (z podagentami w `<sesja>/subagents/`) zmienione
  od początku okna. Wpisy `type: "assistant"` z `timestamp` w oknie.
- Jedna wiadomość = jeden `message.id` + `requestId` (Claude Code zapisuje ten sam wpis z tym
  samym `usage` kilka razy, do 7 razy w sprawdzonym pliku).
- Koszt wiadomości = tokeny × ceny modelu `message.model`: `input_tokens`,
  `cache_creation.ephemeral_5m_input_tokens` i `ephemeral_1h_input_tokens` (gdy brak podziału:
  `cache_creation_input_tokens` po cenie 5m), `cache_read_input_tokens`, `output_tokens`.
  `usage.speed: "fast"` po cenach fast mode. Haiku 5.5 powyżej 100k tokenów promptu po wyższym
  progu.
- Sesja = ścieżka transkryptu (`transcript_path` w `.status` → kafelek); podagenci liczą się do
  sesji-rodzica; transkrypty bez sesji desk idą do „reszta” (po projekcie).
- Czytanie przyrostowe: dla każdego pliku zapamiętany offset i sumy; plik krótszy niż offset
  czytany od nowa. Poza wątkiem UI. Liczone tylko gdy dymek jest otwarty (w zakładkach: na
  „Sesjach”), wynik ważny 30 s.
- Cennik w kodzie, z datą i źródłem (`https://platform.claude.com/docs/en/about-claude/pricing`,
  pobrany 14.11.2026), $ / MTok:

| Model | wejście | zapis 5m | zapis 1h | odczyt cache | wyjście |
|---|---|---|---|---|---|
| Fable 5.2, Mythos 5.2 | 8 | 10 | 16 | 0,20 | 40 |
| Fable 5.1, Mythos 5.1 | 10 | 12,50 | 20 | 0,25 | 50 |
| Fable 5, Mythos 5 | 10 | 12,50 | 20 | 1 | 50 |
| Opus 5.5 | 4 | 5 | 8 | 0,20 | 20 |
| Opus 5, 4.8, 4.7, 4.6, 4.5 | 5 | 6,25 | 10 | 0,50 | 25 |
| Sonnet 5.5 | 2 | 2,50 | 4 | 0,10 | 10 |
| Sonnet 5 | 2 | 2,50 | 4 | 0,20 | 10 |
| Sonnet 4.6, 4.5 | 3 | 3,75 | 6 | 0,30 | 15 |
| Haiku 5.5 (prompt ≤ 100k) | 0,10 | 0,125 | 0,20 | 0,01 | 0,50 |
| Haiku 5.5 (prompt > 100k) | 0,50 | 0,625 | 1 | 0,05 | 2,50 |
| Haiku 4.5 | 1 | 1,25 | 2 | 0,10 | 5 |

  Fast mode (wejście / wyjście): Fable 5.2 i Mythos 5.2: 16 / 80; Opus 5.5: 8 / 40;
  Opus 5 i 4.8: 10 / 50. Model nieznany: najnowszy wiersz tej samej rodziny (fable/mythos, opus,
  sonnet, haiku), a bez rodziny ceny Opus 5.5. Udział to proporcja, więc nieaktualny cennik psuje
  ją tylko przy mieszaniu modeli.

## Opcje: grupa „Statystyki AI”

- Nowa grupa w pasku bocznym Opcji, po „Claude i agenci” (`Group` `"stats"`, pl „Statystyki AI”,
  en „AI stats”).
- „Szczegóły limitów”: [Zakładki] [Wszystko naraz]; `Layout.usageLayout`: `"tabs"` (domyślne)
  | `"all"`; w Rust `usage_layout`.
- „Udział sesji w oknie 5 h”: [Z kosztu sesji desk] [Z transkryptów Claude Code], pod spodem
  podpowiedź z wadą wybranego sposobu; `Layout.usageShare`: `"cost"` (domyślne) |
  `"transcripts"`; w Rust `usage_share`.
- Nieznana wartość w pliku = domyślna. Zmiana działa od razu, także w otwartym dymku.
- Szukajka Opcji znajduje oba wiersze (słowa: limit, statystyki, udział, transkrypt).

## Testy

- Rust:
  - odpowiedź API z 14.11.2026 (powyżej): limity modeli, udziały, kredyty, `severity`;
  - resety ze sztucznego JSON-a zgodnego ze schematem, `null` i nieznane pola;
  - zapis historii: tylko zmiany, przycinanie do 8 dni, uszkodzona linia;
  - udział B: punkt startowy (sesja sprzed okna, z okna, desk uruchomiony w oknie);
  - udział A: duplikaty wpisów, filtr okna, podział zapisu cache 5m/1h, fast mode, próg Haiku 5.5,
    nieznany model, podagenci do rodzica, offset po skróceniu pliku;
  - `layout.json` bez nowych kluczy.
- TS: prognoza (tempo, godzina końca, budżet dzienny, za wcześnie), poziom i kropka licznika
  (przykład z 14.11.2026: czerwony z kropką; bez limitu modelu i prognozy: jak dziś, bez kropki),
  słupki 30 min i resety, przyrosty dzienne, kropka na zakładce, wybór układu przy wąskim oknie.
- Ręcznie: otwieranie i zamykanie, dymek przeżywa odświeżenie paska, tryb dropdown, kilka kont,
  Codex, motyw jasny.

## Zostaje wykonawcy

- Podział na moduły i komendy Tauri (np. jedna `usage_details(agent, account)` zwracająca
  wszystko do dymka albo kilka mniejszych).
- Dokładny wygląd w granicach makiet i palety desk, nazwy klas CSS.
- Kolejność prac i podział na PR-y.
