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
prompt-output-changed-detail = Een ander programma heeft het gewijzigd sinds de samenvoeging openging. Opslaan en stoppen vervangt die wijzigingen door de samenvoeging.
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
stack-inherited = { $path } in { $change } erft zijn conflict van { $origin }. Wie het eerst in { $origin } oplost, lost het ook hier op; een oplossing die hier eerst wordt opgeslagen, geeft waarschijnlijk opnieuw een conflict.
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
menu-copy-as-patch = Als patch kopiëren
menu-copy-difference-as-patch = Verschil als patch kopiëren
menu-toggle-left = Links aan/uit
menu-toggle-base = Basis aan/uit
menu-toggle-right = Rechts aan/uit
menu-clear-choice = Keuze wissen
menu-pick-selected-lines = Geselecteerde regels kiezen
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
menu-unified = Samengevoegd
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
pick-label-inherited = { $title } in gebied { $region } van { $regions }, onopgelost, geërfd
inherited-pill = geërfd
line-pick-label = Regel { $line } van { $title }
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
snapshot-failed-banner = Opgeslagen, maar jj heeft de opslag niet vastgelegd: { $error }
output-changed-banner = { $name } is op de schijf gewijzigd sinds de samenvoeging openging. Opslaan en stoppen vervangt het.
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
diff-pane-unified = Samengevoegd: { $left } tegen { $right }

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
saved =
    { $count ->
        [0] Opgeslagen, opgelost
        [one] Opgeslagen met { $count } conflict
       *[other] Opgeslagen met { $count } conflicten
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

## Error details

err-cannot-read = kan { $path } niet lezen: { $detail }
err-cannot-read-plain = kan { $path } niet lezen
err-cannot-write = kan { $path } niet schrijven: { $detail }
err-cannot-save = kan { $path } niet opslaan: { $detail }
err-cannot-find = kan { $path } niet vinden: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $path } bij { $rev }: { $detail }
err-not-recorded = { $path } is geschreven, maar de repository heeft het opslaan niet vastgelegd: { $detail }
err-save-writing = een opslag schrijft de samenvoeging
err-no-output = de samenvoeging heeft geen uitvoer om te schrijven
err-save-running = er loopt een opslag
err-encode = { $encoding } kan { $character } (U+{ $code }) op regel { $line } niet bevatten
err-binary = binair bestand: een NUL-byte in de eerste { $bytes } bytes
err-invalid-utf8 = ongeldige UTF-8 bij byte { $offset }
err-invalid-utf16 = ongepaard UTF-16-surrogaat bij byte { $offset }
err-odd-utf16 = UTF-16-bestand met een oneven aantal bytes
err-not-a-repository = { $path } ligt niet in een git- of jj-repository
err-not-utf8-path = { $path } is geen UTF-8-pad
err-no-working-copy = { $path } heeft geen werkkopie
err-outside-repository = { $path } ligt buiten de repository
err-stale-file = { $path } op schijf is niet die van de commit van de werkkopie; voer jj workspace update-stale uit en probeer opnieuw
err-stale-workspace = de werkkopie in { $path } is verouderd; voer jj workspace update-stale uit en probeer opnieuw
err-unknown-revision = onbekende revisie { $rev }
err-ambiguous-revision = revisie { $rev } is dubbelzinnig
err-conflicted-at = { $path } heeft conflicten bij { $rev }
err-no-index = { $name } heeft geen index
err-immutable = commit { $rev } is onveranderlijk
err-not-conflicted-at = { $path } heeft geen conflicten bij { $rev }
err-bad-config = { $key } heeft de onbekende waarde { $value }
err-backend = { $context }: { $detail }
err-concurrent-operations = de repository heeft { $count } gelijktijdige bewerkingen; elk jj-commando brengt ze samen
err-not-a-file-conflict = { $path } is geen conflict van bestanden
err-unknown-operation = geen bewerking { $op }: { $reason }
err-no-working-copy-at = de workspace heeft geen werkkopie bij bewerking { $op }
err-no-hunk = geen blok { $hunk }; er zijn er { $count }
err-not-a-destination = { $rev } is geen wijzigbare voorouder van de werkkopie in --into
err-repository-moved = de repository is gewijzigd sinds bewerking { $op }; toon de blokken opnieuw
err-conflict-changed = { $path } in { $change } is gewijzigd sinds triad het las; er wordt niets geschreven
err-sides-changed = { $path }: jj las een conflict van { $after } kanten terug in plaats van een van { $before }
err-jj-config-load = kan de jj-configuratie { $path } niet lezen: { $detail }
err-jj-config-id = { $path }: geen jj-configuratie-id
err-jj-config-value = { $key } in de jj-configuratie: { $detail }
err-jj-config-value-in = { $path }: { $key } in de jj-configuratie: { $detail }
err-jj-config-get = bij het lezen van een jj-configuratiewaarde
err-jj-config-set = bij het instellen van een jj-configuratiewaarde
err-jj-config-find = bij het zoeken naar de jj-configuratie van de repository of workspace
err-auto-track = `snapshot.auto-track = { $text }` is geen fileset
err-git-ignores = bij het lezen van de negeerbestanden van git
err-jj-workspace = bij het laden van de jj-workspace
err-needs-jj = { $command } heeft een jj-repository nodig
err-not-conflicted = { $path } heeft geen conflicten
err-term-not-text = { $path }: { $term } is geen tekst: { $detail }
err-term-not-a-file = { $path }: { $term } is een { $kind }; alleen tekstconflicten zijn op te lossen
err-kind-symlink = symbolische koppeling
err-kind-submodule = submodule
err-kind-directory = map
err-deleted-everywhere = { $path } is aan elke kant verwijderd; niets om samen te voegen
err-resolved-in-tree = { $path } heeft geen conflictmarkeringen meer in de werkboom en lijkt dus opgelost; markeer het als opgelost, of zet de markeringen terug om het opnieuw samen te voegen
err-tree-not-text = { $path }: het bestand in de werkboom is geen tekst ({ $detail }) en kan dus bewerkingen bevatten die een opslag zou overschrijven; zet tekst met de conflictmarkeringen terug, of los het met de hand op
err-temporary = kan geen tijdelijk bestand voor het resultaat maken: { $detail }
err-not-a-directory = { $path } is geen map
err-mixed = diff neemt twee bestanden of twee mappen: { $dir } is een map, { $other } niet
err-git-in-jj = { $path } is een jj-repository, waar triad alleen met de semantiek van jj samenvoegt; los op met `jj resolve`
err-git-in-jj-resolve = { $path } is de oplosmap van jj, waar triad alleen met de semantiek van jj samenvoegt; los op met `jj resolve`
err-no-markers = geen conflictmarkeringen
err-unclosed = regel { $line }: het conflict is niet gesloten
err-nested = regel { $line }: een conflict begint binnen een ander
err-out-of-order = regel { $line }: de markering staat niet op zijn plaats
err-no-separator = regel { $line }: het conflict eindigt vóór zijn =======
err-bad-diff-line = regel { $line }: een diff-regel begint niet met ' ', '-' of '+'
err-marker-shape = regel { $line }: een conflict heeft minstens twee kanten en één basis minder nodig, gevonden { $sides } kanten en { $bases } bases
err-mixed-arity = regel { $line }: een conflict van { $found } kanten na conflicten van { $expected }
err-shape = een samenvoeging heeft minstens twee kanten en één basis minder dan kanten nodig, gevonden { $sides } kanten en { $bases } bases
err-journal-header = het journaal heeft geen volledige kopregel
err-journal-line = regel { $index } van het journaal is niet te lezen: { $detail }
err-journal-version = het journaal heeft formaat { $found }, niet { $expected }
err-journal-inputs = het journaal is voor andere invoer geschreven
err-journal-alignment = het journaal is voor een andere uitlijning van deze invoer geschreven, bijvoorbeeld door een andere versie van triad
err-journal-command = commando { $index } van het journaal is niet toe te passen: { $detail }
err-cannot-open-window = kan het venster niet openen: { $detail }
err-io-not-found = bestand of map bestaat niet
err-io-permission = toegang geweigerd
err-io-exists = het bestand bestaat al
err-io-is-directory = het is een map
err-io-not-directory = geen map
err-io-full = geen ruimte meer op het apparaat
err-io-read-only = het bestandssysteem is alleen-lezen

## gpui-component's text

component-replace = Vervangen
component-replace-all = Alles vervangen
component-clear = Wissen
