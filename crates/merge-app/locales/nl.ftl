# The windows' text in Dutch. `en.ftl` is the reference: it says how counts and names work.

## About window

about-title = Over triad
about-description = Drieweg-samenvoeging en tweewegvergelijking
about-vcs-support = Gemaakt voor Jujutsu, met volledige ondersteuning voor Git
about-version = Versie { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, change { $change }

## Window parts

result-title = Resultaat
merge-toolbar = Samenvoeggereedschap
diff-toolbar = Vergelijkingsgereedschap
input-lines = Invoerregels
go-to-line-field = Regelnummer
no-matches = Geen overeenkomsten
close = Sluiten
settings-errors = Het configuratiebestand bevat fouten: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Opslaan met onopgeloste conflicten?
prompt-save-with-conflicts = Opslaan met conflicten
prompt-keep-editing = Verder bewerken
prompt-discard-detail = Verwerpen schrijft niets, en de samenvoeging blijft onopgelost in git of jj.
prompt-discard = Verwerpen
prompt-cannot-write = Kan de uitvoer niet schrijven
prompt-cannot-write-copy = Kan de kopie niet schrijven
prompt-cannot-record = De repository heeft het opslaan niet vastgelegd
prompt-no-output = Dit venster heeft geen uitvoerbestand.
prompt-quit = Stoppen
prompt-try-again = Opnieuw proberen
prompt-restore-session = De vorige sessie herstellen?
prompt-restore-detail = Een sessie op deze bestanden eindigde voordat de samenvoeging was opgelost. De keuzes en bewerkingen kunnen terugkomen.
prompt-restore = Herstellen
prompt-start-over = Opnieuw beginnen
prompt-cannot-restore = Kan de sessie niet herstellen
prompt-save-changes = Wijzigingen opslaan?
prompt-cannot-save = Kan het bestand niet opslaan
prompt-ok = OK
prompt-changed-detail = Laad het opnieuw en verlies je bewerkingen, of houd je eigen versie en sla die later eroverheen op.
prompt-reload = Opnieuw laden
prompt-keep-mine = Mijn versie houden
prompt-changed-on-disk = { $name } is op schijf gewijzigd.
prompt-descendants-changed =
    { $count ->
        [one] Opslaan wijzigt de conflicten van { $count } afstammeling.
       *[other] Opslaan wijzigt de conflicten van { $count } afstammelingen.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } conflict houdt zijn markeringen, en git of jj zien de samenvoeging als onopgelost.
       *[other] { $count } conflicten houden hun markeringen, en git of jj zien de samenvoeging als onopgelost.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } conflict houdt zijn markeringen, en git of jj zien de samenvoeging als onopgelost. git mergetool zet daarna zijn eigen kopie van het bestand terug; voer het opnieuw uit om deze sessie te herstellen.
       *[other] { $count } conflicten houden hun markeringen, en git of jj zien de samenvoeging als onopgelost. git mergetool zet daarna zijn eigen kopie van het bestand terug; voer het opnieuw uit om deze sessie te herstellen.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } bestand
       *[other] { $count } bestanden
    }
count-versions =
    { $count ->
        [one] { $count } versie
       *[other] { $count } versies
    }
count-hunks =
    { $count ->
        [one] { $count } hunk
       *[other] { $count } hunks
    }
count-differences =
    { $count ->
        [one] { $count } verschil
       *[other] { $count } verschillen
    }
count-conflicts =
    { $count ->
        [one] { $count } conflict
       *[other] { $count } conflicten
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } ongewijzigde regel
       *[other] { $count } ongewijzigde regels
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } dubbel blok
       *[other] { $count } dubbele blokken
    }

## Conflicted commits (stack window)

no-paths = geen
stack-title = Commits met conflicten
stack-empty = Geen commits met conflicten.
refresh = Vernieuwen
stack-preview = Opslaan bekijken
preview-running = Uitzoeken wat opslaan zou doen…
working-copy = werkkopie
no-description = (geen beschrijving)
commit-tooltip =
    Change-ID: { $change }
    Commit-ID: { $commit }
    Datum: { $date }
inherited = (geërfd)
stack-resolved = { $path } opgelost in { $change }.
stack-saved-conflicts = { $path } opgeslagen in { $change } met conflicten.
stack-error = De samenvoeging van { $path } in { $change } eindigde met een fout.
preview-resolves = Opslaan lost het pad op in { $change }.
preview-keeps = Opslaan laat conflicten in { $change }.
preview-no-rebase = Geen afstammeling wordt gerebased.

## Status icons

status-added = Toegevoegd
status-modified = Gewijzigd
status-deleted = Verwijderd
status-moved = Verplaatst
status-copied = Gekopieerd
status-conflicted = Conflict

## Changed files (directory window)

dirs-title = Gewijzigde bestanden
dirs-empty = Geen bestanden verschillen.
dirs-against = { $files } ten opzichte van { $rev }
file-added = toegevoegd
file-deleted = verwijderd
file-modified = gewijzigd
file-renamed = hernoemd van { $from }
file-copied = gekopieerd van { $from }
file-from = van { $from }
file-undecodable = niet te decoderen
file-binary = binair
file-special = speciaal
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ bij operatie { $operation }
no-file = { $name } (geen bestand)

## History window

history-title = Geschiedenis van { $path }
history-empty = Geen versies.
version-conflict = (conflict)
version-absent = (afwezig)
version-other = (anders)

## Absorb window

absorb-title = Absorberen
absorb-empty = Niets te absorberen.
absorb-apply = Toepassen
absorb-keep = Werkkopie
absorb-note = { $hunks } geabsorbeerd in { $commits }.
absorb-stale = De werkkopie is verouderd: voer jj workspace update-stale uit.
absorb-skipped = { $path } overgeslagen: { $reason }

## Open dialog

open-title-merge = Een samenvoeging openen
open-title-diff = Een vergelijking openen
mode-merge = Samenvoegen
mode-diff = Vergelijken
start-merge = Samenvoegen
start-diff = Vergelijken
slot-base = Basis
slot-left = Links
slot-right = Rechts
slot-output = Uitvoer
choose-base = Het basisbestand kiezen
choose-left = Het linkerbestand kiezen
choose-right = Het rechterbestand kiezen
choose-output = Het uitvoerbestand kiezen
missing-slot = Kies een bestand voor ‘{ $slot }’.

## Menus

menu-settings = Instellingen…
menu-save = Opslaan
menu-save-and-exit = Opslaan en stoppen
menu-save-copy-as = Kopie opslaan als…
menu-cancel = Annuleren
menu-edit = Wijzig
menu-merge = Samenvoegen
menu-go = Ga
menu-view = Weergave
menu-undo = Herstel
menu-redo = Opnieuw
menu-find = Zoek…
menu-go-to-line = Ga naar regel…
menu-toggle-left = Links aan/uit
menu-toggle-base = Basis aan/uit
menu-toggle-right = Rechts aan/uit
menu-clear-choice = Keuze wissen
menu-take-base = Basis nemen
menu-take-left = Links nemen
menu-take-right = Rechts nemen
menu-take-left-then-right = Links en dan rechts nemen
menu-take-right-then-left = Rechts en dan links nemen
menu-take-sorted-union = Gesorteerde vereniging nemen
menu-take-left-all-unresolved = Links nemen voor alle onopgeloste
menu-take-right-all-unresolved = Rechts nemen voor alle onopgeloste
menu-take-whole-left = Het hele linkerbestand nemen
menu-take-whole-right = Het hele rechterbestand nemen
menu-carry-moved-edit = Bewerking naar verplaatst blok meenemen
menu-drop-duplicate = Dubbel verwijderen
menu-next-difference = Volgend verschil
menu-previous-difference = Vorig verschil
menu-next-conflict = Volgend conflict
menu-previous-conflict = Vorig conflict
menu-go-to-move-end = Naar het andere eind van de verplaatsing
menu-base-pane = Basispaneel
menu-blame = Herkomst
menu-syntax = Syntaxiskleuring
menu-soft-wrap = Regelomloop
menu-whitespace = Witruimte tonen
menu-collapse-unchanged = Ongewijzigd invouwen
menu-scroll-sync = Resultaat synchroon scrollen

## Toolbar

tool-label-difference = Verschil
tool-label-conflict = Conflict
tool-label-left = Links
tool-label-right = Rechts
more = Meer
more-menu = Meer keuzes
more-base = Basis
more-left-then-right = Links en dan rechts
more-right-then-left = Rechts en dan links
more-none = Geen
tool-previous-difference = Vorig verschil
tool-next-difference = Volgend verschil
tool-previous-conflict = Vorig conflict
tool-next-conflict = Volgend conflict
tool-left = Links aan/uit in het huidige gebied
tool-base = Basis aan/uit in het huidige gebied
tool-right = Rechts aan/uit in het huidige gebied
tool-left-then-right = Links en dan rechts nemen in het huidige gebied
tool-right-then-left = Rechts en dan links nemen in het huidige gebied
tool-clear = De keuze van het huidige gebied wissen
tool-undo = Herstel
tool-redo = Opnieuw
tool-base-pane = Het basispaneel tonen of verbergen
tool-find = Zoek
tool-go-to-line = Naar een regel in het resultaat
tool-syntax = Syntaxiskleuren aan- of uitzetten
tool-soft-wrap = Regelomloop aan- of uitzetten
tool-whitespace = Witruimte tonen of verbergen

## Conflict badge and pick targets

pick-label = { $title } in gebied { $region } van { $regions }
pick-label-unresolved = { $title } in gebied { $region } van { $regions }, onopgelost
badge-conflict = Conflict { $number } van { $count }
badge-sides = { $kept } van { $sides } kanten
badge-all-resolved = alle opgelost
badge-resolved-one = Conflict opgelost
badge-resolved-all = Alle { $count } opgelost
badge-unresolved-of = { $unresolved } van { $count } onopgelost

## Panes

role-left = Links
role-right = Rechts
role-base = Basis
role-side = Kant { $number }
role-base-of = Basis { $number }
role-result = Resultaat
cell-line = { $title }, regel { $line }: { $text }
cell-no-line = { $title }, geen regel
cell-no-line-deleted = { $title }, geen regel: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Niets gekozen ({ $count } regel weggelaten)
       *[other] Niets gekozen ({ $count } regels weggelaten)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } regel verwijderd ({ $roles })
       *[other] { $count } regels verwijderd ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } regel verwijderd
       *[other] { $count } regels verwijderd
    }
previous-match = Vorige overeenkomst in { $title }
next-match = Volgende overeenkomst in { $title }
match-count = { $number } van { $count }
go-to-line-prompt = Ga naar regel (1–{ $last }):
go-to-line-current = Huidige regel { $line } van { $last }
go-to-line-target = Ga naar regel { $line }
jj-diff-banner = Dit zijn tijdelijke kopieën van jj diff --tool. Bewerkingen hier gaan verloren als het venster sluit.
copy-to-right = Dit verschil naar rechts kopiëren
copy-to-left = Dit verschil naar links kopiëren
initial-base = B
initial-left = L
initial-right = R
moved-to-base = Verplaatst blok: naar het andere eind, basis regel { $line }
moved-to-left = Verplaatst blok: naar het andere eind, links regel { $line }
moved-to-right = Verplaatst blok: naar het andere eind, rechts regel { $line }
diff-pane-left = Links: { $name }
diff-pane-right = Rechts: { $name }

## Status bar

repo-git = git-repo
repo-jj = jj-repo
repo-none = geen repo
mode-git = git-samenvoeging
mode-jj = jj-samenvoeging
mode-git-word = git-samenvoeging per woord
mode-jj-word = jj-samenvoeging per woord
encoding-utf8-bom = UTF-8 met BOM
inputs-differ = { $encoding } (invoer verschilt)
word-merged = { $count } per woord samengevoegd
unresolved = { $count } onopgelost
mixed-endings = gemengde regeleinden
compare-whitespace = witruimte telt
compare-line-endings = regeleinden genegeerd
compare-trailing = witruimte aan regeleinde genegeerd
compare-amount = hoeveelheid witruimte genegeerd
compare-all = alle witruimte genegeerd
case-ignored = hoofdletters genegeerd
patterns-ignored = patronen genegeerd
blank-lines-ignored = lege regels genegeerd
diverge-whitespace-jj = witruimte anders vergeleken dan jj
diverge-case-git = hoofdletters genegeerd, anders dan git
diverge-case-jj = hoofdletters genegeerd, anders dan jj
diverge-patterns-git = patronen genegeerd, anders dan git
diverge-patterns-jj = patronen genegeerd, anders dan jj
diverge-same-change-git = gelijke wijzigingen behouden, anders dan git
diverge-word-git = woorden samengevoegd, anders dan git
diverge-deep-git = conflicten binnenin gesplitst, anders dan git
diverge-deep-jj = conflicten binnenin gesplitst, anders dan jj
diverge-syntax-git = syntaxisgrenzen behouden, anders dan git
diverge-syntax-jj = syntaxisgrenzen behouden, anders dan jj
copy-saved =
    { $count ->
        [0] Kopie opgeslagen, opgelost
        [one] Kopie opgeslagen met { $count } conflict
       *[other] Kopie opgeslagen met { $count } conflicten
    }

## Result pane's right-click menu

choice-none = Geen
choice-base = Basis
choice-left = Links
choice-right = Rechts
choice-left-then-right = Links + rechts
choice-right-then-left = Rechts + links
action-sorted-union = Gesorteerde vereniging
suggested = Voorstel: { $action }
suggested-carry = Voorstel: bewerking van { $from } meenemen naar regel { $line }
suggested-drop = Voorstel: dubbel van regel { $line } verwijderen
edit-cut = Knip
edit-copy = Kopieer
edit-paste = Plak
edit-select-all = Selecteer alles
