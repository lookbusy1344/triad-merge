# The windows' text in French. `en.ftl` is the reference: it says how counts and names work.
# French counts 0 as singular (`[one]`).

## About window

about-title = À propos de triad
about-description = Fusion à trois voies et comparaison à deux voies
about-vcs-support = Conçu pour Jujutsu, avec une prise en charge complète de Git
about-version = Version { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, changement { $change }

## Window parts

result-title = Résultat
merge-toolbar = Outils de fusion
diff-toolbar = Outils de comparaison
input-lines = Lignes d’entrée
go-to-line-field = Numéro de ligne
no-matches = Aucun résultat
close = Fermer
settings-errors = Le fichier de configuration contient des erreurs : { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Enregistrer avec des conflits non résolus ?
prompt-save-with-conflicts = Enregistrer avec les conflits
prompt-keep-editing = Continuer l’édition
prompt-discard-detail = Abandonner n’écrit rien, et la fusion reste non résolue dans git ou jj.
prompt-discard = Abandonner
prompt-cannot-write = Impossible d’écrire la sortie
prompt-cannot-write-copy = Impossible d’écrire la copie
prompt-cannot-record = Le dépôt n’a pas enregistré la sauvegarde
prompt-no-output = Cette fenêtre n’a pas de fichier de sortie.
prompt-quit = Quitter
prompt-try-again = Réessayer
prompt-restore-session = Restaurer la session précédente ?
prompt-restore-detail = Une session sur ces fichiers s’est terminée avant la résolution de sa fusion. Ses choix et modifications peuvent être récupérés.
prompt-restore = Restaurer
prompt-start-over = Recommencer
prompt-cannot-restore = Impossible de restaurer la session
prompt-save-changes = Enregistrer les modifications ?
prompt-cannot-save = Impossible d’enregistrer le fichier
prompt-ok = OK
prompt-changed-detail = Rechargez-le et perdez vos modifications, ou gardez votre version et enregistrez-la par-dessus plus tard.
prompt-reload = Recharger
prompt-output-changed-detail = Un autre programme l’a modifié depuis l’ouverture de la fusion. Enregistrer et quitter remplace ces modifications par la fusion.
prompt-keep-mine = Garder la mienne
prompt-changed-on-disk = { $name } a changé sur le disque.
prompt-descendants-changed =
    { $count ->
        [one] L’enregistrement modifie les conflits de { $count } descendant.
       *[other] L’enregistrement modifie les conflits de { $count } descendants.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } conflit garde ses marqueurs, et git ou jj considèrent la fusion comme non résolue.
       *[other] { $count } conflits gardent leurs marqueurs, et git ou jj considèrent la fusion comme non résolue.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } conflit garde ses marqueurs, et git ou jj considèrent la fusion comme non résolue. git mergetool remet ensuite sa propre copie du fichier ; relancez-le pour restaurer cette session.
       *[other] { $count } conflits gardent leurs marqueurs, et git ou jj considèrent la fusion comme non résolue. git mergetool remet ensuite sa propre copie du fichier ; relancez-le pour restaurer cette session.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } fichier
       *[other] { $count } fichiers
    }
count-versions =
    { $count ->
        [one] { $count } version
       *[other] { $count } versions
    }
count-hunks =
    { $count ->
        [one] { $count } section
       *[other] { $count } sections
    }
count-differences =
    { $count ->
        [one] { $count } différence
       *[other] { $count } différences
    }
count-conflicts =
    { $count ->
        [one] { $count } conflit
       *[other] { $count } conflits
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } ligne inchangée
       *[other] { $count } lignes inchangées
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } bloc dupliqué
       *[other] { $count } blocs dupliqués
    }

## Conflicted commits (stack window)

no-paths = aucun
stack-title = Commits en conflit
stack-empty = Aucun commit en conflit.
refresh = Actualiser
stack-preview = Aperçu de l’enregistrement
preview-running = Calcul de l’effet de l’enregistrement…
working-copy = copie de travail
no-description = (aucune description)
commit-tooltip =
    ID de changement : { $change }
    ID de commit : { $commit }
    Date : { $date }
inherited = (hérité)
stack-resolved = { $path } résolu dans { $change }.
stack-saved-conflicts = { $path } enregistré dans { $change } avec des conflits.
stack-error = La fusion de { $path } dans { $change } s’est terminée par une erreur.
stack-inherited = { $path } dans { $change } hérite son conflit de { $origin }. Le résoudre d’abord dans { $origin } le fait disparaître ici ; une résolution enregistrée ici d’abord entrera probablement de nouveau en conflit.
preview-resolves = L’enregistrement résout le chemin dans { $change }.
preview-keeps = L’enregistrement laisse des conflits dans { $change }.
preview-no-rebase = Aucun descendant n’est rebasé.

## Status icons

status-added = Ajouté
status-modified = Modifié
status-deleted = Supprimé
status-moved = Déplacé
status-copied = Copié
status-conflicted = En conflit

## Changed files (directory window)

dirs-title = Fichiers modifiés
dirs-empty = Aucun fichier ne diffère.
dirs-against = { $files } par rapport à { $rev }
file-added = ajouté
file-deleted = supprimé
file-modified = modifié
file-renamed = renommé depuis { $from }
file-copied = copié depuis { $from }
file-from = depuis { $from }
file-undecodable = non décodable
file-binary = binaire
file-special = spécial
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ à l’opération { $operation }
no-file = { $name } (aucun fichier)

## History window

history-title = Historique de { $path }
history-empty = Aucune version.
version-conflict = (conflit)
version-absent = (absent)
version-other = (autre)

## Absorb window

absorb-title = Absorber
absorb-empty = Rien à absorber.
absorb-apply = Appliquer
absorb-keep = Copie de travail
absorb-note = Absorption de { $hunks } dans { $commits }.
absorb-stale = La copie de travail est périmée : lancez jj workspace update-stale.
absorb-skipped = { $path } ignoré : { $reason }

## Open dialog

open-title-merge = Ouvrir une fusion
open-title-diff = Ouvrir une comparaison
mode-merge = Fusion
mode-diff = Comparaison
start-merge = Fusionner
start-diff = Comparer
slot-base = Base
slot-left = Gauche
slot-right = Droite
slot-output = Sortie
choose-base = Choisir le fichier de base
choose-left = Choisir le fichier de gauche
choose-right = Choisir le fichier de droite
choose-output = Choisir le fichier de sortie
missing-slot = Choisissez un fichier pour « { $slot } ».

## Menus

menu-settings = Réglages…
menu-save = Enregistrer
menu-save-and-exit = Enregistrer et quitter
menu-save-copy-as = Enregistrer une copie sous…
menu-cancel = Annuler
menu-edit = Édition
menu-merge = Fusion
menu-go = Aller
menu-view = Présentation
menu-undo = Annuler l’action
menu-redo = Rétablir
menu-find = Rechercher…
menu-go-to-line = Aller à la ligne…
menu-copy-as-patch = Copier comme patch
menu-copy-difference-as-patch = Copier la différence comme patch
menu-toggle-left = Basculer gauche
menu-toggle-base = Basculer base
menu-toggle-right = Basculer droite
menu-clear-choice = Effacer le choix
menu-pick-selected-lines = Choisir les lignes sélectionnées
menu-take-base = Prendre la base
menu-take-left = Prendre gauche
menu-take-right = Prendre droite
menu-take-left-then-right = Prendre gauche puis droite
menu-take-right-then-left = Prendre droite puis gauche
menu-take-sorted-union = Prendre l’union triée
menu-take-left-all-unresolved = Prendre gauche pour tous les non résolus
menu-take-right-all-unresolved = Prendre droite pour tous les non résolus
menu-take-whole-left = Prendre tout le fichier de gauche
menu-take-whole-right = Prendre tout le fichier de droite
menu-carry-moved-edit = Reporter la modification sur le bloc déplacé
menu-drop-duplicate = Supprimer le doublon
menu-next-difference = Différence suivante
menu-previous-difference = Différence précédente
menu-next-conflict = Conflit suivant
menu-previous-conflict = Conflit précédent
menu-go-to-move-end = Aller à l’autre bout du déplacement
menu-base-pane = Panneau de base
menu-blame = Origine des lignes
menu-syntax = Coloration syntaxique
menu-soft-wrap = Retour à la ligne
menu-whitespace = Afficher les espaces
menu-collapse-unchanged = Replier l’inchangé
menu-unified = Unifiée
menu-scroll-sync = Synchroniser le défilement du résultat

## Toolbar

tool-label-difference = Différence
tool-label-conflict = Conflit
tool-label-left = Gauche
tool-label-right = Droite
more = Plus
more-menu = Autres choix
more-base = Base
more-left-then-right = Gauche puis droite
more-right-then-left = Droite puis gauche
more-none = Aucun
tool-previous-difference = Différence précédente
tool-next-difference = Différence suivante
tool-previous-conflict = Conflit précédent
tool-next-conflict = Conflit suivant
tool-left = Basculer gauche dans la région courante
tool-base = Basculer base dans la région courante
tool-right = Basculer droite dans la région courante
tool-left-then-right = Prendre gauche puis droite dans la région courante
tool-right-then-left = Prendre droite puis gauche dans la région courante
tool-clear = Effacer le choix de la région courante
tool-undo = Annuler l’action
tool-redo = Rétablir
tool-base-pane = Afficher ou masquer le panneau de base
tool-find = Rechercher
tool-go-to-line = Aller à une ligne du résultat
tool-syntax = Activer ou désactiver les couleurs syntaxiques
tool-soft-wrap = Activer ou désactiver le retour à la ligne
tool-whitespace = Afficher ou masquer les espaces

## Conflict badge and pick targets

pick-label = { $title } dans la région { $region } sur { $regions }
pick-label-unresolved = { $title } dans la région { $region } sur { $regions }, non résolu
pick-label-inherited = { $title } dans la région { $region } sur { $regions }, non résolu, hérité
inherited-pill = hérité
line-pick-label = Ligne { $line } de { $title }
badge-conflict = Conflit { $number } sur { $count }
badge-sides = { $kept } côtés sur { $sides }
badge-all-resolved = tous résolus
badge-resolved-one = Conflit résolu
badge-resolved-all = Les { $count } résolus
badge-unresolved-of = { $unresolved } sur { $count } non résolus

## Panes

role-left = Gauche
role-right = Droite
role-base = Base
role-side = Côté { $number }
role-base-of = Base { $number }
role-result = Résultat
cell-line = { $title }, ligne { $line } : { $text }
cell-no-line = { $title }, aucune ligne
cell-no-line-deleted = { $title }, aucune ligne : { $deleted }
placeholder-nothing =
    { $count ->
        [one] Rien de choisi ({ $count } ligne abandonnée)
       *[other] Rien de choisi ({ $count } lignes abandonnées)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } ligne supprimée ({ $roles })
       *[other] { $count } lignes supprimées ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } ligne supprimée
       *[other] { $count } lignes supprimées
    }
previous-match = Résultat précédent dans { $title }
next-match = Résultat suivant dans { $title }
match-count = { $number } sur { $count }
go-to-line-prompt = Aller à la ligne (1–{ $last }) :
go-to-line-current = Ligne actuelle { $line } sur { $last }
go-to-line-target = Aller à la ligne { $line }
jj-diff-banner = Ce sont des copies temporaires de jj diff --tool. Les modifications faites ici sont perdues à la fermeture de la fenêtre.
snapshot-failed-banner = Enregistré, mais jj n’a pas consigné l’enregistrement : { $error }
output-changed-banner = { $name } a changé sur le disque depuis l’ouverture de la fusion. Enregistrer et quitter le remplace.
copy-to-right = Copier cette différence à droite
copy-to-left = Copier cette différence à gauche
initial-base = B
initial-left = G
initial-right = D
moved-to-base = Bloc déplacé : aller à son autre bout, ligne { $line } de la base
moved-to-left = Bloc déplacé : aller à son autre bout, ligne { $line } de gauche
moved-to-right = Bloc déplacé : aller à son autre bout, ligne { $line } de droite
diff-pane-left = Gauche : { $name }
diff-pane-right = Droite : { $name }
diff-pane-unified = Unifiée : { $left } contre { $right }

## Status bar

repo-git = dépôt git
repo-jj = dépôt jj
repo-none = aucun dépôt
mode-git = fusion git
mode-jj = fusion jj
mode-git-word = fusion git par mot
mode-jj-word = fusion jj par mot
encoding-utf8-bom = UTF-8 avec BOM
encoding-utf16le-bom = UTF-16 LE avec BOM
encoding-utf16be-bom = UTF-16 BE avec BOM
inputs-differ = { $encoding } (les entrées diffèrent)
word-merged =
    { $count ->
        [one] { $count } fusionné par mot
       *[other] { $count } fusionnés par mot
    }
unresolved =
    { $count ->
        [one] { $count } non résolu
       *[other] { $count } non résolus
    }
mixed-endings = fins de ligne mixtes
compare-whitespace = espaces significatifs
compare-line-endings = fins de ligne ignorées
compare-trailing = espaces de fin ignorés
compare-amount = quantité d’espaces ignorée
compare-all = tous les espaces ignorés
case-ignored = casse ignorée
patterns-ignored = motifs ignorés
blank-lines-ignored = lignes vides ignorées
diverge-whitespace-jj = espaces comparés autrement que jj
diverge-case-git = casse ignorée, contrairement à git
diverge-case-jj = casse ignorée, contrairement à jj
diverge-patterns-git = motifs ignorés, contrairement à git
diverge-patterns-jj = motifs ignorés, contrairement à jj
diverge-same-change-git = changements identiques gardés, contrairement à git
diverge-word-git = mots fusionnés, contrairement à git
diverge-deep-git = conflits divisés à l’intérieur, contrairement à git
diverge-deep-jj = conflits divisés à l’intérieur, contrairement à jj
diverge-syntax-git = limites syntaxiques respectées, contrairement à git
diverge-syntax-jj = limites syntaxiques respectées, contrairement à jj
copy-saved =
    { $count ->
        [0] Copie enregistrée, résolue
        [one] Copie enregistrée avec { $count } conflit
       *[other] Copie enregistrée avec { $count } conflits
    }
saved =
    { $count ->
        [0] Enregistré, résolu
        [one] Enregistré avec { $count } conflit
       *[other] Enregistré avec { $count } conflits
    }

## Result pane's right-click menu

choice-none = Aucun
choice-base = Base
choice-left = Gauche
choice-right = Droite
choice-left-then-right = Gauche + droite
choice-right-then-left = Droite + gauche
action-sorted-union = Union triée
suggested = Suggestion : { $action }
suggested-carry = Suggestion : reporter la modification de { $from } à la ligne { $line }
suggested-drop = Suggestion : supprimer le doublon de la ligne { $line }
edit-cut = Couper
edit-copy = Copier
edit-paste = Coller
edit-select-all = Tout sélectionner

## Error details

err-cannot-read = impossible de lire { $path } : { $detail }
err-cannot-read-plain = impossible de lire { $path }
err-cannot-write = impossible d’écrire { $path } : { $detail }
err-cannot-save = impossible d’enregistrer { $path } : { $detail }
err-cannot-find = impossible de trouver { $path } : { $detail }
err-in = { $path } : { $detail }
err-at-rev = { $path } à { $rev } : { $detail }
err-not-recorded = { $path } est écrit, mais le dépôt n’a pas enregistré la sauvegarde : { $detail }
err-save-writing = une sauvegarde écrit la fusion
err-no-output = la fusion n’a pas de sortie à écrire
err-save-running = une sauvegarde est en cours
err-encode = { $encoding } ne peut pas contenir { $character } (U+{ $code }) à la ligne { $line }
err-binary = fichier binaire : un octet NUL dans les { $bytes } premiers octets
err-invalid-utf8 = UTF-8 non valide à l’octet { $offset }
err-invalid-utf16 = substitut UTF-16 non apparié à l’octet { $offset }
err-odd-utf16 = fichier UTF-16 avec un nombre impair d’octets
err-not-a-repository = { $path } n’est dans aucun dépôt git ou jj
err-not-utf8-path = { $path } n’est pas un chemin UTF-8
err-no-working-copy = { $path } n’a pas de copie de travail
err-outside-repository = { $path } est hors du dépôt
err-stale-file = { $path } sur le disque n’est pas celui du commit de la copie de travail ; lancez jj workspace update-stale et réessayez
err-stale-workspace = la copie de travail dans { $path } est périmée ; lancez jj workspace update-stale et réessayez
err-unknown-revision = révision inconnue { $rev }
err-ambiguous-revision = la révision { $rev } est ambiguë
err-conflicted-at = { $path } est en conflit à { $rev }
err-no-index = { $name } n’a pas d’index
err-immutable = le commit { $rev } est immuable
err-not-conflicted-at = { $path } n’est pas en conflit à { $rev }
err-bad-config = { $key } a la valeur inconnue { $value }
err-backend = { $context } : { $detail }
err-concurrent-operations = le dépôt a { $count } opérations concurrentes ; n’importe quelle commande jj les réconcilie
err-not-a-file-conflict = { $path } n’est pas un conflit de fichiers
err-unknown-operation = aucune opération { $op } : { $reason }
err-no-working-copy-at = l’espace de travail n’a pas de copie de travail à l’opération { $op }
err-no-hunk = aucun bloc { $hunk } ; il y en a { $count }
err-not-a-destination = { $rev } n’est pas un ancêtre modifiable de la copie de travail dans --into
err-repository-moved = le dépôt a changé depuis l’opération { $op } ; listez de nouveau les blocs
err-conflict-changed = { $path } dans { $change } a changé depuis que triad l’a lu ; rien n’est écrit
err-sides-changed = { $path } : jj a relu un conflit de { $after } côtés au lieu d’un de { $before }
err-jj-config-load = impossible de lire la configuration jj { $path } : { $detail }
err-jj-config-id = { $path } : pas un identifiant de configuration jj
err-jj-config-value = { $key } dans la configuration jj : { $detail }
err-jj-config-value-in = { $path } : { $key } dans la configuration jj : { $detail }
err-jj-config-get = lecture d’une valeur de configuration jj
err-jj-config-set = écriture d’une valeur de configuration jj
err-jj-config-find = recherche de la configuration jj du dépôt ou de l’espace de travail
err-auto-track = `snapshot.auto-track = { $text }` n’est pas un fileset
err-git-ignores = lecture des fichiers d’exclusion de git
err-jj-workspace = chargement de l’espace de travail jj
err-needs-jj = { $command } nécessite un dépôt jj
err-not-conflicted = { $path } n’est pas en conflit
err-term-not-text = { $path } : { $term } n’est pas du texte : { $detail }
err-term-not-a-file = { $path } : { $term } est un { $kind } ; seuls les conflits de texte peuvent être résolus
err-kind-symlink = lien symbolique
err-kind-submodule = sous-module
err-kind-directory = répertoire
err-deleted-everywhere = { $path } est supprimé de tous les côtés ; rien à fusionner
err-resolved-in-tree = { $path } n’a plus de marqueurs de conflit dans l’arbre de travail et semble donc résolu ; marquez-le comme résolu, ou remettez ses marqueurs pour le fusionner de nouveau
err-tree-not-text = { $path } : le fichier de l’arbre de travail n’est pas du texte ({ $detail }) et peut donc contenir des modifications qu’une sauvegarde écraserait ; remettez du texte avec ses marqueurs de conflit, ou résolvez-le à la main
err-not-a-directory = { $path } n’est pas un répertoire
err-mixed = diff prend deux fichiers ou deux répertoires : { $dir } est un répertoire, { $other } non
err-git-in-jj = { $path } est un dépôt jj, où triad fusionne uniquement avec la sémantique de jj ; résolvez avec `jj resolve`
err-git-in-jj-resolve = { $path } est le répertoire de résolution de jj, où triad fusionne uniquement avec la sémantique de jj ; résolvez avec `jj resolve`
err-no-markers = aucun marqueur de conflit
err-unclosed = ligne { $line } : le conflit n’est pas fermé
err-nested = ligne { $line } : un conflit commence dans un autre
err-out-of-order = ligne { $line } : le marqueur n’est pas à sa place
err-no-separator = ligne { $line } : le conflit se termine avant son =======
err-bad-diff-line = ligne { $line } : une ligne de diff ne commence ni par ' ', ni par '-', ni par '+'
err-marker-shape = ligne { $line } : un conflit exige au moins deux côtés et une base de moins, trouvé { $sides } côtés et { $bases } bases
err-mixed-arity = ligne { $line } : un conflit de { $found } côtés après des conflits de { $expected }
err-shape = une fusion exige au moins deux côtés et une base de moins que de côtés, trouvé { $sides } côtés et { $bases } bases
err-journal-header = le journal n’a pas de ligne d’en-tête complète
err-journal-line = la ligne { $index } du journal ne s’analyse pas : { $detail }
err-journal-version = le journal a le format { $found }, pas { $expected }
err-journal-inputs = le journal a été écrit pour d’autres entrées
err-journal-alignment = le journal a été écrit pour un autre alignement de ces entrées, par exemple celui d’une autre version de triad
err-journal-command = la commande { $index } du journal ne s’applique pas : { $detail }
err-cannot-open-window = impossible d’ouvrir la fenêtre : { $detail }
err-io-not-found = fichier ou répertoire introuvable
err-io-permission = permission refusée
err-io-exists = le fichier existe déjà
err-io-is-directory = c’est un répertoire
err-io-not-directory = ce n’est pas un répertoire
err-io-full = plus d’espace sur le périphérique
err-io-read-only = le système de fichiers est en lecture seule

## gpui-component's text

component-replace = Remplacer
component-replace-all = Tout remplacer
component-clear = Effacer
