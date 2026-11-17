# The windows' text in Polish. `en.ftl` is the reference: it says how counts and names work.
# Polish counts take three forms for whole numbers: `[one]` (1 alone), `[few]` (2–4, 22–24…,
# but not 12–14) and `[many]` (0, 5–21, 25–31…). `[many]` is the default.

## About window

about-title = O programie triad
about-description = Scalanie trójstronne i porównanie dwustronne
about-vcs-support = Stworzony dla Jujutsu, z pełną obsługą Git
about-version = Wersja { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, zmiana { $change }

## Window parts

result-title = Wynik
merge-toolbar = Narzędzia scalania
diff-toolbar = Narzędzia porównania
input-lines = Wiersze wejściowe
go-to-line-field = Numer wiersza
no-matches = Brak dopasowań
close = Zamknij
settings-errors = Plik ustawień zawiera błędy: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Zapisać z nierozwiązanymi konfliktami?
prompt-save-with-conflicts = Zapisz z konfliktami
prompt-keep-editing = Edytuj dalej
prompt-discard-detail = Odrzucenie niczego nie zapisuje, a scalanie pozostaje nierozwiązane w git lub jj.
prompt-discard = Odrzuć
prompt-cannot-write = Nie można zapisać wyniku
prompt-cannot-write-copy = Nie można zapisać kopii
prompt-cannot-record = Repozytorium nie zarejestrowało zapisu
prompt-no-output = To okno nie ma pliku wyjściowego.
prompt-quit = Zakończ
prompt-try-again = Spróbuj ponownie
prompt-restore-session = Przywrócić poprzednią sesję?
prompt-restore-detail = Sesja na tych plikach zakończyła się przed rozwiązaniem scalania. Jej wybory i edycje mogą wrócić.
prompt-restore = Przywróć
prompt-start-over = Zacznij od nowa
prompt-cannot-restore = Nie można przywrócić sesji
prompt-save-changes = Zapisać zmiany?
prompt-cannot-save = Nie można zapisać pliku
prompt-ok = OK
prompt-changed-detail = Wczytaj go ponownie i utrać edycje albo zachowaj swoją wersję i zapisz ją na nim później.
prompt-reload = Wczytaj ponownie
prompt-keep-mine = Zachowaj moją
prompt-changed-on-disk = Plik { $name } zmienił się na dysku.
prompt-descendants-changed =
    { $count ->
        [one] Zapis zmienia konflikty { $count } potomka.
       *[many] Zapis zmienia konflikty { $count } potomków.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } konflikt zachowuje swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane.
        [few] { $count } konflikty zachowują swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane.
       *[many] { $count } konfliktów zachowuje swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } konflikt zachowuje swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane. Potem git mergetool przywróci własną kopię pliku; uruchom go ponownie, aby przywrócić tę sesję.
        [few] { $count } konflikty zachowują swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane. Potem git mergetool przywróci własną kopię pliku; uruchom go ponownie, aby przywrócić tę sesję.
       *[many] { $count } konfliktów zachowuje swoje znaczniki, a git lub jj uznają scalanie za nierozwiązane. Potem git mergetool przywróci własną kopię pliku; uruchom go ponownie, aby przywrócić tę sesję.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
        [few] { $count } commity
       *[many] { $count } commitów
    }
count-files =
    { $count ->
        [one] { $count } plik
        [few] { $count } pliki
       *[many] { $count } plików
    }
count-versions =
    { $count ->
        [one] { $count } wersja
        [few] { $count } wersje
       *[many] { $count } wersji
    }
count-hunks =
    { $count ->
        [one] { $count } fragment
        [few] { $count } fragmenty
       *[many] { $count } fragmentów
    }
count-differences =
    { $count ->
        [one] { $count } różnica
        [few] { $count } różnice
       *[many] { $count } różnic
    }
count-conflicts =
    { $count ->
        [one] { $count } konflikt
        [few] { $count } konflikty
       *[many] { $count } konfliktów
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } niezmieniony wiersz
        [few] { $count } niezmienione wiersze
       *[many] { $count } niezmienionych wierszy
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } powielony blok
        [few] { $count } powielone bloki
       *[many] { $count } powielonych bloków
    }

## Conflicted commits (stack window)

no-paths = brak
stack-title = Commity z konfliktami
stack-empty = Brak commitów z konfliktami.
refresh = Odśwież
stack-preview = Podgląd zapisu
preview-running = Ustalanie skutków zapisu…
working-copy = kopia robocza
no-description = (brak opisu)
commit-tooltip =
    ID zmiany: { $change }
    ID commitu: { $commit }
    Data: { $date }
inherited = (odziedziczony)
stack-resolved = Rozwiązano { $path } w { $change }.
stack-saved-conflicts = Zapisano { $path } w { $change } z konfliktami.
stack-error = Scalanie { $path } w { $change } zakończyło się błędem.
preview-resolves = Zapis rozwiąże ścieżkę w { $change }.
preview-keeps = Po zapisie w { $change } zostaną konflikty.
preview-no-rebase = Żaden potomek nie zostanie przebazowany.

## Status icons

status-added = Dodany
status-modified = Zmieniony
status-deleted = Usunięty
status-moved = Przeniesiony
status-copied = Skopiowany
status-conflicted = Konflikt

## Changed files (directory window)

dirs-title = Zmienione pliki
dirs-empty = Żadne pliki się nie różnią.
dirs-against = { $files } względem { $rev }
file-added = dodany
file-deleted = usunięty
file-modified = zmieniony
file-renamed = zmieniono nazwę z { $from }
file-copied = skopiowany z { $from }
file-from = z { $from }
file-undecodable = nie do odkodowania
file-binary = binarny
file-special = specjalny
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ przy operacji { $operation }
no-file = { $name } (brak pliku)

## History window

history-title = Historia { $path }
history-empty = Brak wersji.
version-conflict = (konflikt)
version-absent = (brak)
version-other = (inna)

## Absorb window

absorb-title = Wchłanianie
absorb-empty = Nie ma czego wchłonąć.
absorb-apply = Zastosuj
absorb-keep = Kopia robocza
absorb-note = Wchłonięto { $hunks } do { $commits }.
absorb-stale = Kopia robocza jest nieaktualna: uruchom jj workspace update-stale.
absorb-skipped = Pominięto { $path }: { $reason }

## Open dialog

open-title-merge = Otwórz scalanie
open-title-diff = Otwórz porównanie
mode-merge = Scal
mode-diff = Porównaj
start-merge = Scal
start-diff = Porównaj
slot-base = Baza
slot-left = Lewa
slot-right = Prawa
slot-output = Wynik
choose-base = Wybierz plik bazowy
choose-left = Wybierz lewy plik
choose-right = Wybierz prawy plik
choose-output = Wybierz plik wyjściowy
missing-slot = Wybierz plik dla „{ $slot }”.

## Menus

menu-settings = Ustawienia…
menu-save = Zapisz
menu-save-and-exit = Zapisz i zakończ
menu-save-copy-as = Zapisz kopię jako…
menu-cancel = Anuluj
menu-edit = Edycja
menu-merge = Scalanie
menu-go = Przejdź
menu-view = Widok
menu-undo = Cofnij
menu-redo = Ponów
menu-find = Znajdź…
menu-go-to-line = Przejdź do wiersza…
menu-toggle-left = Przełącz lewą
menu-toggle-base = Przełącz bazę
menu-toggle-right = Przełącz prawą
menu-clear-choice = Wyczyść wybór
menu-take-base = Weź bazę
menu-take-left = Weź lewą
menu-take-right = Weź prawą
menu-take-left-then-right = Weź lewą, potem prawą
menu-take-right-then-left = Weź prawą, potem lewą
menu-take-sorted-union = Weź posortowaną sumę
menu-take-left-all-unresolved = Weź lewą we wszystkich nierozwiązanych
menu-take-right-all-unresolved = Weź prawą we wszystkich nierozwiązanych
menu-take-whole-left = Weź cały lewy plik
menu-take-whole-right = Weź cały prawy plik
menu-carry-moved-edit = Przenieś edycję do przeniesionego bloku
menu-drop-duplicate = Usuń duplikat
menu-next-difference = Następna różnica
menu-previous-difference = Poprzednia różnica
menu-next-conflict = Następny konflikt
menu-previous-conflict = Poprzedni konflikt
menu-go-to-move-end = Do drugiego końca przeniesienia
menu-base-pane = Panel bazy
menu-blame = Pochodzenie
menu-syntax = Podświetlanie składni
menu-soft-wrap = Zawijanie wierszy
menu-whitespace = Pokaż białe znaki
menu-collapse-unchanged = Zwiń niezmienione
menu-scroll-sync = Synchronizuj przewijanie wyniku

## Toolbar

tool-label-difference = Różnica
tool-label-conflict = Konflikt
tool-label-left = Lewa
tool-label-right = Prawa
more = Więcej
more-menu = Więcej wyborów
more-base = Baza
more-left-then-right = Lewa, potem prawa
more-right-then-left = Prawa, potem lewa
more-none = Nic
tool-previous-difference = Poprzednia różnica
tool-next-difference = Następna różnica
tool-previous-conflict = Poprzedni konflikt
tool-next-conflict = Następny konflikt
tool-left = Przełącz lewą w bieżącym obszarze
tool-base = Przełącz bazę w bieżącym obszarze
tool-right = Przełącz prawą w bieżącym obszarze
tool-left-then-right = Weź lewą, potem prawą w bieżącym obszarze
tool-right-then-left = Weź prawą, potem lewą w bieżącym obszarze
tool-clear = Wyczyść wybór bieżącego obszaru
tool-undo = Cofnij
tool-redo = Ponów
tool-base-pane = Pokaż lub ukryj panel bazy
tool-find = Znajdź
tool-go-to-line = Przejdź do wiersza wyniku
tool-syntax = Włącz lub wyłącz kolory składni
tool-soft-wrap = Włącz lub wyłącz zawijanie wierszy
tool-whitespace = Pokaż lub ukryj białe znaki

## Conflict badge and pick targets

pick-label = { $title } w obszarze { $region } z { $regions }
pick-label-unresolved = { $title } w obszarze { $region } z { $regions }, nierozwiązany
badge-conflict = Konflikt { $number } z { $count }
badge-sides = { $kept } z { $sides } stron
badge-all-resolved = wszystkie rozwiązane
badge-resolved-one = Konflikt rozwiązany
badge-resolved-all = Wszystkie rozwiązane: { $count }
badge-unresolved-of = Nierozwiązane: { $unresolved } z { $count }

## Panes

role-left = Lewa
role-right = Prawa
role-base = Baza
role-side = Strona { $number }
role-base-of = Baza { $number }
role-result = Wynik
cell-line = { $title }, wiersz { $line }: { $text }
cell-no-line = { $title }, brak wiersza
cell-no-line-deleted = { $title }, brak wiersza: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Nic nie wybrano (pominięto { $count } wiersz)
        [few] Nic nie wybrano (pominięto { $count } wiersze)
       *[many] Nic nie wybrano (pominięto { $count } wierszy)
    }
placeholder-deleted =
    { $count ->
        [one] Usunięto { $count } wiersz ({ $roles })
        [few] Usunięto { $count } wiersze ({ $roles })
       *[many] Usunięto { $count } wierszy ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] Usunięto { $count } wiersz
        [few] Usunięto { $count } wiersze
       *[many] Usunięto { $count } wierszy
    }
previous-match = Poprzednie dopasowanie w { $title }
next-match = Następne dopasowanie w { $title }
match-count = { $number } z { $count }
go-to-line-prompt = Przejdź do wiersza (1–{ $last }):
go-to-line-current = Bieżący wiersz { $line } z { $last }
go-to-line-target = Przejdź do wiersza { $line }
jj-diff-banner = To tymczasowe kopie z jj diff --tool. Edycje tutaj przepadną po zamknięciu okna.
copy-to-right = Skopiuj tę różnicę na prawo
copy-to-left = Skopiuj tę różnicę na lewo
initial-base = B
initial-left = L
initial-right = P
moved-to-base = Przeniesiony blok: do drugiego końca, baza wiersz { $line }
moved-to-left = Przeniesiony blok: do drugiego końca, lewa wiersz { $line }
moved-to-right = Przeniesiony blok: do drugiego końca, prawa wiersz { $line }
diff-pane-left = Lewa: { $name }
diff-pane-right = Prawa: { $name }

## Status bar

repo-git = repozytorium git
repo-jj = repozytorium jj
repo-none = brak repozytorium
mode-git = scalanie git
mode-jj = scalanie jj
mode-git-word = scalanie git po słowach
mode-jj-word = scalanie jj po słowach
encoding-utf8-bom = UTF-8 z BOM
inputs-differ = { $encoding } (wejścia się różnią)
word-merged = Scalone po słowach: { $count }
unresolved = Nierozwiązane: { $count }
mixed-endings = mieszane końce wierszy
compare-whitespace = białe znaki mają znaczenie
compare-line-endings = końce wierszy pomijane
compare-trailing = białe znaki na końcu wierszy pomijane
compare-amount = liczba białych znaków pomijana
compare-all = wszystkie białe znaki pomijane
case-ignored = wielkość liter pomijana
patterns-ignored = wzorce pomijane
blank-lines-ignored = puste wiersze pomijane
diverge-whitespace-jj = białe znaki porównywane inaczej niż w jj
diverge-case-git = wielkość liter pomijana, inaczej niż w git
diverge-case-jj = wielkość liter pomijana, inaczej niż w jj
diverge-patterns-git = wzorce pomijane, inaczej niż w git
diverge-patterns-jj = wzorce pomijane, inaczej niż w jj
diverge-same-change-git = takie same zmiany zachowane, inaczej niż w git
diverge-word-git = słowa scalone, inaczej niż w git
diverge-deep-git = konflikty podzielone wewnątrz, inaczej niż w git
diverge-deep-jj = konflikty podzielone wewnątrz, inaczej niż w jj
diverge-syntax-git = granice składni zachowane, inaczej niż w git
diverge-syntax-jj = granice składni zachowane, inaczej niż w jj
copy-saved =
    { $count ->
        [0] Kopia zapisana, rozwiązana
        [one] Kopia zapisana z { $count } konfliktem
       *[many] Kopia zapisana z konfliktami: { $count }
    }

## Result pane's right-click menu

choice-none = Nic
choice-base = Baza
choice-left = Lewa
choice-right = Prawa
choice-left-then-right = Lewa + prawa
choice-right-then-left = Prawa + lewa
action-sorted-union = Posortowana suma
suggested = Sugestia: { $action }
suggested-carry = Sugestia: przenieś edycję z { $from } do wiersza { $line }
suggested-drop = Sugestia: usuń duplikat wiersza { $line }
edit-cut = Wytnij
edit-copy = Kopiuj
edit-paste = Wklej
edit-select-all = Zaznacz wszystko
