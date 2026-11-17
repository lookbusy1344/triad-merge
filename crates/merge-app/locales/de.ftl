# The windows' text in German. `en.ftl` is the reference: it says how counts and names work.

## About window

about-title = Über triad
about-description = Drei-Wege-Merge und Zwei-Wege-Vergleich
about-vcs-support = Für Jujutsu gebaut, mit voller Git-Unterstützung
about-version = Version { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, Change { $change }

## Window parts

result-title = Ergebnis
merge-toolbar = Merge-Werkzeuge
diff-toolbar = Vergleichswerkzeuge
input-lines = Eingabezeilen
go-to-line-field = Zeilennummer
no-matches = Keine Treffer
close = Schließen
settings-errors = Die Konfigurationsdatei hat Fehler: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Mit ungelösten Konflikten speichern?
prompt-save-with-conflicts = Mit Konflikten speichern
prompt-keep-editing = Weiter bearbeiten
prompt-discard-detail = Verwerfen schreibt nichts, und der Merge bleibt in git oder jj ungelöst.
prompt-discard = Verwerfen
prompt-cannot-write = Die Ausgabe kann nicht geschrieben werden
prompt-cannot-write-copy = Die Kopie kann nicht geschrieben werden
prompt-cannot-record = Das Repository hat das Speichern nicht aufgezeichnet
prompt-no-output = Dieses Fenster hat keine Ausgabedatei.
prompt-quit = Beenden
prompt-try-again = Erneut versuchen
prompt-restore-session = Die vorige Sitzung wiederherstellen?
prompt-restore-detail = Eine Sitzung zu diesen Dateien endete, bevor ihr Merge gelöst war. Ihre Auswahlen und Bearbeitungen lassen sich wiederherstellen.
prompt-restore = Wiederherstellen
prompt-start-over = Neu beginnen
prompt-cannot-restore = Die Sitzung kann nicht wiederhergestellt werden
prompt-save-changes = Änderungen speichern?
prompt-cannot-save = Die Datei kann nicht gespeichert werden
prompt-ok = OK
prompt-changed-detail = Neu laden und die Bearbeitungen verlieren, oder die eigene Fassung behalten und sie später darüber speichern.
prompt-reload = Neu laden
prompt-keep-mine = Meine behalten
prompt-changed-on-disk = { $name } wurde auf der Festplatte geändert.
prompt-descendants-changed =
    { $count ->
        [one] Speichern ändert die Konflikte von { $count } Nachfolger.
       *[other] Speichern ändert die Konflikte von { $count } Nachfolgern.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } Konflikt behält seine Markierungen, und git oder jj sehen den Merge als ungelöst.
       *[other] { $count } Konflikte behalten ihre Markierungen, und git oder jj sehen den Merge als ungelöst.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } Konflikt behält seine Markierungen, und git oder jj sehen den Merge als ungelöst. git mergetool legt danach seine eigene Kopie der Datei zurück; ein erneuter Aufruf stellt diese Sitzung wieder her.
       *[other] { $count } Konflikte behalten ihre Markierungen, und git oder jj sehen den Merge als ungelöst. git mergetool legt danach seine eigene Kopie der Datei zurück; ein erneuter Aufruf stellt diese Sitzung wieder her.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } Commit
       *[other] { $count } Commits
    }
count-files =
    { $count ->
        [one] { $count } Datei
       *[other] { $count } Dateien
    }
count-versions =
    { $count ->
        [one] { $count } Version
       *[other] { $count } Versionen
    }
count-hunks =
    { $count ->
        [one] { $count } Hunk
       *[other] { $count } Hunks
    }
count-differences =
    { $count ->
        [one] { $count } Unterschied
       *[other] { $count } Unterschiede
    }
count-conflicts =
    { $count ->
        [one] { $count } Konflikt
       *[other] { $count } Konflikte
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } unveränderte Zeile
       *[other] { $count } unveränderte Zeilen
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } doppelter Block
       *[other] { $count } doppelte Blöcke
    }

## Conflicted commits (stack window)

no-paths = keine
stack-title = Commits mit Konflikten
stack-empty = Keine Commits mit Konflikten.
refresh = Aktualisieren
stack-preview = Speichern vorschauen
preview-running = Ermittle, was das Speichern bewirkt…
working-copy = Arbeitskopie
no-description = (keine Beschreibung)
commit-tooltip =
    Change-ID: { $change }
    Commit-ID: { $commit }
    Datum: { $date }
inherited = (geerbt)
stack-resolved = { $path } in { $change } gelöst.
stack-saved-conflicts = { $path } in { $change } mit Konflikten gespeichert.
stack-error = Der Merge von { $path } in { $change } endete mit einem Fehler.
preview-resolves = Speichern löst den Pfad in { $change }.
preview-keeps = Speichern lässt Konflikte in { $change }.
preview-no-rebase = Kein Nachfolger wird umgesetzt.

## Status icons

status-added = Hinzugefügt
status-modified = Geändert
status-deleted = Gelöscht
status-moved = Verschoben
status-copied = Kopiert
status-conflicted = Konflikt

## Changed files (directory window)

dirs-title = Geänderte Dateien
dirs-empty = Keine Dateien unterscheiden sich.
dirs-against = { $files } gegenüber { $rev }
file-added = hinzugefügt
file-deleted = gelöscht
file-modified = geändert
file-renamed = umbenannt von { $from }
file-copied = kopiert von { $from }
file-from = von { $from }
file-undecodable = nicht dekodierbar
file-binary = binär
file-special = speziell
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ bei Operation { $operation }
no-file = { $name } (keine Datei)

## History window

history-title = Verlauf von { $path }
history-empty = Keine Versionen.
version-conflict = (Konflikt)
version-absent = (fehlt)
version-other = (anderes)

## Absorb window

absorb-title = Absorbieren
absorb-empty = Nichts zu absorbieren.
absorb-apply = Anwenden
absorb-keep = Arbeitskopie
absorb-note = { $hunks } in { $commits } absorbiert.
absorb-stale = Die Arbeitskopie ist veraltet: jj workspace update-stale ausführen.
absorb-skipped = { $path } übersprungen: { $reason }

## Open dialog

open-title-merge = Merge öffnen
open-title-diff = Vergleich öffnen
mode-merge = Merge
mode-diff = Vergleich
start-merge = Mergen
start-diff = Vergleichen
slot-base = Basis
slot-left = Links
slot-right = Rechts
slot-output = Ausgabe
choose-base = Basisdatei wählen
choose-left = Linke Datei wählen
choose-right = Rechte Datei wählen
choose-output = Ausgabedatei wählen
missing-slot = Eine Datei für „{ $slot }“ wählen.

## Menus

menu-settings = Einstellungen…
menu-save = Speichern
menu-save-and-exit = Speichern und beenden
menu-save-copy-as = Kopie speichern unter…
menu-cancel = Abbrechen
menu-edit = Bearbeiten
menu-merge = Merge
menu-go = Gehe zu
menu-view = Darstellung
menu-undo = Widerrufen
menu-redo = Wiederholen
menu-find = Suchen…
menu-go-to-line = Gehe zu Zeile…
menu-toggle-left = Links umschalten
menu-toggle-base = Basis umschalten
menu-toggle-right = Rechts umschalten
menu-clear-choice = Auswahl löschen
menu-take-base = Basis übernehmen
menu-take-left = Links übernehmen
menu-take-right = Rechts übernehmen
menu-take-left-then-right = Links, dann rechts übernehmen
menu-take-right-then-left = Rechts, dann links übernehmen
menu-take-sorted-union = Sortierte Vereinigung übernehmen
menu-take-left-all-unresolved = Links für alle ungelösten übernehmen
menu-take-right-all-unresolved = Rechts für alle ungelösten übernehmen
menu-take-whole-left = Ganze linke Datei übernehmen
menu-take-whole-right = Ganze rechte Datei übernehmen
menu-carry-moved-edit = Bearbeitung in verschobenen Block übertragen
menu-drop-duplicate = Duplikat entfernen
menu-next-difference = Nächster Unterschied
menu-previous-difference = Vorheriger Unterschied
menu-next-conflict = Nächster Konflikt
menu-previous-conflict = Vorheriger Konflikt
menu-go-to-move-end = Zum anderen Ende der Verschiebung
menu-base-pane = Basisbereich
menu-blame = Herkunft
menu-syntax = Syntaxhervorhebung
menu-soft-wrap = Zeilenumbruch
menu-whitespace = Leerraum zeigen
menu-collapse-unchanged = Unverändertes einklappen
menu-scroll-sync = Ergebnis synchron scrollen

## Toolbar

tool-label-difference = Unterschied
tool-label-conflict = Konflikt
tool-label-left = Links
tool-label-right = Rechts
more = Mehr
more-menu = Weitere Auswahlen
more-base = Basis
more-left-then-right = Links, dann rechts
more-right-then-left = Rechts, dann links
more-none = Keine
tool-previous-difference = Vorheriger Unterschied
tool-next-difference = Nächster Unterschied
tool-previous-conflict = Vorheriger Konflikt
tool-next-conflict = Nächster Konflikt
tool-left = Links im aktuellen Bereich umschalten
tool-base = Basis im aktuellen Bereich umschalten
tool-right = Rechts im aktuellen Bereich umschalten
tool-left-then-right = Links, dann rechts im aktuellen Bereich übernehmen
tool-right-then-left = Rechts, dann links im aktuellen Bereich übernehmen
tool-clear = Auswahl des aktuellen Bereichs löschen
tool-undo = Widerrufen
tool-redo = Wiederholen
tool-base-pane = Basisbereich zeigen oder ausblenden
tool-find = Suchen
tool-go-to-line = Zu einer Zeile im Ergebnis gehen
tool-syntax = Syntaxfarben ein- oder ausschalten
tool-soft-wrap = Zeilenumbruch ein- oder ausschalten
tool-whitespace = Leerraum zeigen oder ausblenden

## Conflict badge and pick targets

pick-label = { $title } in Bereich { $region } von { $regions }
pick-label-unresolved = { $title } in Bereich { $region } von { $regions }, ungelöst
badge-conflict = Konflikt { $number } von { $count }
badge-sides = { $kept } von { $sides } Seiten
badge-all-resolved = alle gelöst
badge-resolved-one = Konflikt gelöst
badge-resolved-all = Alle { $count } gelöst
badge-unresolved-of = { $unresolved } von { $count } ungelöst

## Panes

role-left = Links
role-right = Rechts
role-base = Basis
role-side = Seite { $number }
role-base-of = Basis { $number }
role-result = Ergebnis
cell-line = { $title }, Zeile { $line }: { $text }
cell-no-line = { $title }, keine Zeile
cell-no-line-deleted = { $title }, keine Zeile: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Nichts gewählt ({ $count } Zeile verworfen)
       *[other] Nichts gewählt ({ $count } Zeilen verworfen)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } Zeile gelöscht ({ $roles })
       *[other] { $count } Zeilen gelöscht ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } Zeile gelöscht
       *[other] { $count } Zeilen gelöscht
    }
previous-match = Vorheriger Treffer in { $title }
next-match = Nächster Treffer in { $title }
match-count = { $number } von { $count }
go-to-line-prompt = Gehe zu Zeile (1–{ $last }):
go-to-line-current = Aktuelle Zeile { $line } von { $last }
go-to-line-target = Gehe zu Zeile { $line }
jj-diff-banner = Dies sind temporäre Kopien von jj diff --tool. Bearbeitungen hier gehen verloren, wenn das Fenster schließt.
copy-to-right = Diesen Unterschied nach rechts kopieren
copy-to-left = Diesen Unterschied nach links kopieren
initial-base = B
initial-left = L
initial-right = R
moved-to-base = Verschobener Block: zum anderen Ende, Basis Zeile { $line }
moved-to-left = Verschobener Block: zum anderen Ende, links Zeile { $line }
moved-to-right = Verschobener Block: zum anderen Ende, rechts Zeile { $line }
diff-pane-left = Links: { $name }
diff-pane-right = Rechts: { $name }

## Status bar

repo-git = git-Repo
repo-jj = jj-Repo
repo-none = kein Repo
mode-git = git-Merge
mode-jj = jj-Merge
mode-git-word = git-Merge nach Wörtern
mode-jj-word = jj-Merge nach Wörtern
encoding-utf8-bom = UTF-8 mit BOM
inputs-differ = { $encoding } (Eingaben unterscheiden sich)
word-merged = { $count } nach Wörtern gemergt
unresolved = { $count } ungelöst
mixed-endings = gemischte Zeilenenden
compare-whitespace = Leerraum zählt
compare-line-endings = Zeilenenden ignoriert
compare-trailing = Leerraum am Zeilenende ignoriert
compare-amount = Leerraummenge ignoriert
compare-all = jeder Leerraum ignoriert
case-ignored = Groß-/Kleinschreibung ignoriert
patterns-ignored = Muster ignoriert
blank-lines-ignored = Leerzeilen ignoriert
diverge-whitespace-jj = Leerraum anders als in jj verglichen
diverge-case-git = Groß-/Kleinschreibung ignoriert, anders als git
diverge-case-jj = Groß-/Kleinschreibung ignoriert, anders als jj
diverge-patterns-git = Muster ignoriert, anders als git
diverge-patterns-jj = Muster ignoriert, anders als jj
diverge-same-change-git = gleiche Änderungen behalten, anders als git
diverge-word-git = Wörter gemergt, anders als git
diverge-deep-git = Konflikte innen geteilt, anders als git
diverge-deep-jj = Konflikte innen geteilt, anders als jj
diverge-syntax-git = Syntaxgrenzen eingehalten, anders als git
diverge-syntax-jj = Syntaxgrenzen eingehalten, anders als jj
copy-saved =
    { $count ->
        [0] Kopie gespeichert, gelöst
        [one] Kopie mit { $count } Konflikt gespeichert
       *[other] Kopie mit { $count } Konflikten gespeichert
    }

## Result pane's right-click menu

choice-none = Keine
choice-base = Basis
choice-left = Links
choice-right = Rechts
choice-left-then-right = Links + rechts
choice-right-then-left = Rechts + links
action-sorted-union = Sortierte Vereinigung
suggested = Vorschlag: { $action }
suggested-carry = Vorschlag: Bearbeitung von { $from } in Zeile { $line } übertragen
suggested-drop = Vorschlag: Duplikat von Zeile { $line } entfernen
edit-cut = Ausschneiden
edit-copy = Kopieren
edit-paste = Einsetzen
edit-select-all = Alles auswählen
