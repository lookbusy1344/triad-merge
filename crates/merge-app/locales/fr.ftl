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
menu-toggle-left = Basculer gauche
menu-toggle-base = Basculer base
menu-toggle-right = Basculer droite
menu-clear-choice = Effacer le choix
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

## Status bar

repo-git = dépôt git
repo-jj = dépôt jj
repo-none = aucun dépôt
mode-git = fusion git
mode-jj = fusion jj
mode-git-word = fusion git par mot
mode-jj-word = fusion jj par mot
encoding-utf8-bom = UTF-8 avec BOM
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
