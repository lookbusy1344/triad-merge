# The windows' text in Swedish. `en.ftl` is the reference: it says how counts and names work.

## About window

about-title = Om triad
about-description = Trevägssammanslagning och tvåvägsjämförelse
about-vcs-support = Byggt för Jujutsu, med fullt stöd för Git
about-version = Version { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, ändring { $change }

## Window parts

result-title = Resultat
merge-toolbar = Sammanslagningsverktyg
diff-toolbar = Jämförelseverktyg
input-lines = Indatarader
go-to-line-field = Radnummer
no-matches = Inga träffar
close = Stäng
settings-errors = Inställningsfilen har fel: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Spara med olösta konflikter?
prompt-save-with-conflicts = Spara med konflikter
prompt-keep-editing = Fortsätt redigera
prompt-discard-detail = Kassera skriver ingenting, och sammanslagningen förblir olöst i git eller jj.
prompt-discard = Kassera
prompt-cannot-write = Kan inte skriva utdata
prompt-cannot-write-copy = Kan inte skriva kopian
prompt-cannot-record = Förrådet registrerade inte sparandet
prompt-no-output = Det här fönstret har ingen utdatafil.
prompt-quit = Avsluta
prompt-try-again = Försök igen
prompt-restore-session = Återställa förra sessionen?
prompt-restore-detail = En session med de här filerna slutade innan sammanslagningen var löst. Dess val och redigeringar kan komma tillbaka.
prompt-restore = Återställ
prompt-start-over = Börja om
prompt-cannot-restore = Kan inte återställa sessionen
prompt-save-changes = Spara ändringarna?
prompt-cannot-save = Kan inte spara filen
prompt-ok = OK
prompt-changed-detail = Läs in den igen och förlora dina redigeringar, eller behåll din version och spara över den senare.
prompt-reload = Läs in igen
prompt-keep-mine = Behåll min
prompt-changed-on-disk = { $name } har ändrats på disken.
prompt-descendants-changed =
    { $count ->
        [one] Att spara ändrar konflikterna hos { $count } ättling.
       *[other] Att spara ändrar konflikterna hos { $count } ättlingar.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } konflikt behåller sina markeringar, och git eller jj ser sammanslagningen som olöst.
       *[other] { $count } konflikter behåller sina markeringar, och git eller jj ser sammanslagningen som olöst.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } konflikt behåller sina markeringar, och git eller jj ser sammanslagningen som olöst. git mergetool lägger sedan tillbaka sin egen kopia av filen; kör det igen för att återställa den här sessionen.
       *[other] { $count } konflikter behåller sina markeringar, och git eller jj ser sammanslagningen som olöst. git mergetool lägger sedan tillbaka sin egen kopia av filen; kör det igen för att återställa den här sessionen.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } fil
       *[other] { $count } filer
    }
count-versions =
    { $count ->
        [one] { $count } version
       *[other] { $count } versioner
    }
count-hunks =
    { $count ->
        [one] { $count } stycke
       *[other] { $count } stycken
    }
count-differences =
    { $count ->
        [one] { $count } skillnad
       *[other] { $count } skillnader
    }
count-conflicts =
    { $count ->
        [one] { $count } konflikt
       *[other] { $count } konflikter
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } oförändrad rad
       *[other] { $count } oförändrade rader
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } dubblerat block
       *[other] { $count } dubblerade block
    }

## Conflicted commits (stack window)

no-paths = inga
stack-title = Commits med konflikter
stack-empty = Inga commits med konflikter.
refresh = Uppdatera
stack-preview = Förhandsgranska sparande
preview-running = Räknar ut vad sparandet gör…
working-copy = arbetskopia
no-description = (ingen beskrivning)
commit-tooltip =
    Ändrings-ID: { $change }
    Commit-ID: { $commit }
    Datum: { $date }
inherited = (ärvd)
stack-resolved = { $path } löst i { $change }.
stack-saved-conflicts = { $path } sparad i { $change } med konflikter.
stack-error = Sammanslagningen av { $path } i { $change } slutade med ett fel.
preview-resolves = Att spara löser sökvägen i { $change }.
preview-keeps = Att spara lämnar konflikter i { $change }.
preview-no-rebase = Ingen ättling flyttas om.

## Status icons

status-added = Tillagd
status-modified = Ändrad
status-deleted = Borttagen
status-moved = Flyttad
status-copied = Kopierad
status-conflicted = Konflikt

## Changed files (directory window)

dirs-title = Ändrade filer
dirs-empty = Inga filer skiljer sig.
dirs-against = { $files } mot { $rev }
file-added = tillagd
file-deleted = borttagen
file-modified = ändrad
file-renamed = omdöpt från { $from }
file-copied = kopierad från { $from }
file-from = från { $from }
file-undecodable = kan inte avkodas
file-binary = binär
file-special = särskild
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ vid operation { $operation }
no-file = { $name } (ingen fil)

## History window

history-title = Historik för { $path }
history-empty = Inga versioner.
version-conflict = (konflikt)
version-absent = (saknas)
version-other = (annan)

## Absorb window

absorb-title = Absorbera
absorb-empty = Inget att absorbera.
absorb-apply = Verkställ
absorb-keep = Arbetskopia
absorb-note = { $hunks } absorberade i { $commits }.
absorb-stale = Arbetskopian är inaktuell: kör jj workspace update-stale.
absorb-skipped = { $path } hoppades över: { $reason }

## Open dialog

open-title-merge = Öppna en sammanslagning
open-title-diff = Öppna en jämförelse
mode-merge = Slå samman
mode-diff = Jämför
start-merge = Slå samman
start-diff = Jämför
slot-base = Bas
slot-left = Vänster
slot-right = Höger
slot-output = Utdata
choose-base = Välj basfilen
choose-left = Välj vänster fil
choose-right = Välj höger fil
choose-output = Välj utdatafilen
missing-slot = Välj en fil för ”{ $slot }”.

## Menus

menu-settings = Inställningar…
menu-save = Spara
menu-save-and-exit = Spara och avsluta
menu-save-copy-as = Spara kopia som…
menu-cancel = Avbryt
menu-edit = Redigera
menu-merge = Sammanslagning
menu-go = Gå
menu-view = Visa
menu-undo = Ångra
menu-redo = Gör om
menu-find = Sök…
menu-go-to-line = Gå till rad…
menu-toggle-left = Växla vänster
menu-toggle-base = Växla bas
menu-toggle-right = Växla höger
menu-clear-choice = Rensa val
menu-take-base = Ta bas
menu-take-left = Ta vänster
menu-take-right = Ta höger
menu-take-left-then-right = Ta vänster, sedan höger
menu-take-right-then-left = Ta höger, sedan vänster
menu-take-sorted-union = Ta sorterad union
menu-take-left-all-unresolved = Ta vänster för alla olösta
menu-take-right-all-unresolved = Ta höger för alla olösta
menu-take-whole-left = Ta hela vänstra filen
menu-take-whole-right = Ta hela högra filen
menu-carry-moved-edit = För redigeringen till det flyttade blocket
menu-drop-duplicate = Ta bort dubblett
menu-next-difference = Nästa skillnad
menu-previous-difference = Föregående skillnad
menu-next-conflict = Nästa konflikt
menu-previous-conflict = Föregående konflikt
menu-go-to-move-end = Gå till flyttens andra ände
menu-base-pane = Baspanel
menu-blame = Ursprung
menu-syntax = Syntaxmarkering
menu-soft-wrap = Radbrytning
menu-whitespace = Visa blanktecken
menu-collapse-unchanged = Fäll ihop oförändrat
menu-scroll-sync = Synkronisera resultatets rullning

## Toolbar

tool-label-difference = Skillnad
tool-label-conflict = Konflikt
tool-label-left = Vänster
tool-label-right = Höger
more = Mer
more-menu = Fler val
more-base = Bas
more-left-then-right = Vänster, sedan höger
more-right-then-left = Höger, sedan vänster
more-none = Inget
tool-previous-difference = Föregående skillnad
tool-next-difference = Nästa skillnad
tool-previous-conflict = Föregående konflikt
tool-next-conflict = Nästa konflikt
tool-left = Växla vänster i aktuellt område
tool-base = Växla bas i aktuellt område
tool-right = Växla höger i aktuellt område
tool-left-then-right = Ta vänster, sedan höger i aktuellt område
tool-right-then-left = Ta höger, sedan vänster i aktuellt område
tool-clear = Rensa valet i aktuellt område
tool-undo = Ångra
tool-redo = Gör om
tool-base-pane = Visa eller dölj baspanelen
tool-find = Sök
tool-go-to-line = Gå till en rad i resultatet
tool-syntax = Slå på eller av syntaxfärger
tool-soft-wrap = Slå på eller av radbrytning
tool-whitespace = Visa eller dölj blanktecken

## Conflict badge and pick targets

pick-label = { $title } i område { $region } av { $regions }
pick-label-unresolved = { $title } i område { $region } av { $regions }, olöst
badge-conflict = Konflikt { $number } av { $count }
badge-sides = { $kept } av { $sides } sidor
badge-all-resolved = alla lösta
badge-resolved-one = Konflikten löst
badge-resolved-all = Alla { $count } lösta
badge-unresolved-of = { $unresolved } av { $count } olösta

## Panes

role-left = Vänster
role-right = Höger
role-base = Bas
role-side = Sida { $number }
role-base-of = Bas { $number }
role-result = Resultat
cell-line = { $title }, rad { $line }: { $text }
cell-no-line = { $title }, ingen rad
cell-no-line-deleted = { $title }, ingen rad: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Inget valt ({ $count } rad utelämnad)
       *[other] Inget valt ({ $count } rader utelämnade)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } rad borttagen ({ $roles })
       *[other] { $count } rader borttagna ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } rad borttagen
       *[other] { $count } rader borttagna
    }
previous-match = Föregående träff i { $title }
next-match = Nästa träff i { $title }
match-count = { $number } av { $count }
go-to-line-prompt = Gå till rad (1–{ $last }):
go-to-line-current = Aktuell rad { $line } av { $last }
go-to-line-target = Gå till rad { $line }
jj-diff-banner = Det här är tillfälliga kopior från jj diff --tool. Redigeringar här går förlorade när fönstret stängs.
copy-to-right = Kopiera den här skillnaden till höger
copy-to-left = Kopiera den här skillnaden till vänster
initial-base = B
initial-left = V
initial-right = H
moved-to-base = Flyttat block: gå till andra änden, bas rad { $line }
moved-to-left = Flyttat block: gå till andra änden, vänster rad { $line }
moved-to-right = Flyttat block: gå till andra änden, höger rad { $line }
diff-pane-left = Vänster: { $name }
diff-pane-right = Höger: { $name }

## Status bar

repo-git = git-förråd
repo-jj = jj-förråd
repo-none = inget förråd
mode-git = git-sammanslagning
mode-jj = jj-sammanslagning
mode-git-word = git-sammanslagning per ord
mode-jj-word = jj-sammanslagning per ord
encoding-utf8-bom = UTF-8 med BOM
inputs-differ = { $encoding } (indata skiljer sig)
word-merged = { $count } sammanslagna per ord
unresolved = { $count } olösta
mixed-endings = blandade radslut
compare-whitespace = blanktecken räknas
compare-line-endings = radslut ignoreras
compare-trailing = blanktecken i radslut ignoreras
compare-amount = mängden blanktecken ignoreras
compare-all = alla blanktecken ignoreras
case-ignored = skiftläge ignoreras
patterns-ignored = mönster ignoreras
blank-lines-ignored = tomma rader ignoreras
diverge-whitespace-jj = blanktecken jämförs annorlunda än jj
diverge-case-git = skiftläge ignoreras, till skillnad från git
diverge-case-jj = skiftläge ignoreras, till skillnad från jj
diverge-patterns-git = mönster ignoreras, till skillnad från git
diverge-patterns-jj = mönster ignoreras, till skillnad från jj
diverge-same-change-git = samma ändringar behålls, till skillnad från git
diverge-word-git = ord sammanslagna, till skillnad från git
diverge-deep-git = konflikter delade inuti, till skillnad från git
diverge-deep-jj = konflikter delade inuti, till skillnad från jj
diverge-syntax-git = syntaxgränser behållna, till skillnad från git
diverge-syntax-jj = syntaxgränser behållna, till skillnad från jj
copy-saved =
    { $count ->
        [0] Kopia sparad, löst
        [one] Kopia sparad med { $count } konflikt
       *[other] Kopia sparad med { $count } konflikter
    }

## Result pane's right-click menu

choice-none = Inget
choice-base = Bas
choice-left = Vänster
choice-right = Höger
choice-left-then-right = Vänster + höger
choice-right-then-left = Höger + vänster
action-sorted-union = Sorterad union
suggested = Förslag: { $action }
suggested-carry = Förslag: för redigeringen från { $from } till rad { $line }
suggested-drop = Förslag: ta bort dubbletten av rad { $line }
edit-cut = Klipp ut
edit-copy = Kopiera
edit-paste = Klistra in
edit-select-all = Markera alla
