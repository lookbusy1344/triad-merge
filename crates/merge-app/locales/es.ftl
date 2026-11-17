# The windows' text in Spanish. `en.ftl` is the reference: it says how counts and names work.

## About window

about-title = Acerca de triad
about-description = Fusión de tres vías y comparación de dos vías
about-vcs-support = Hecho para Jujutsu, con soporte completo de Git
about-version = Versión { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, cambio { $change }

## Window parts

result-title = Resultado
merge-toolbar = Herramientas de fusión
diff-toolbar = Herramientas de comparación
input-lines = Líneas de entrada
go-to-line-field = Número de línea
no-matches = Sin coincidencias
close = Cerrar
settings-errors = El archivo de configuración tiene errores: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = ¿Guardar con conflictos sin resolver?
prompt-save-with-conflicts = Guardar con conflictos
prompt-keep-editing = Seguir editando
prompt-discard-detail = Descartar no escribe nada, y la fusión sigue sin resolver en git o jj.
prompt-discard = Descartar
prompt-cannot-write = No se puede escribir la salida
prompt-cannot-write-copy = No se puede escribir la copia
prompt-cannot-record = El repositorio no registró el guardado
prompt-no-output = Esta ventana no tiene archivo de salida.
prompt-quit = Salir
prompt-try-again = Reintentar
prompt-restore-session = ¿Restaurar la sesión anterior?
prompt-restore-detail = Una sesión sobre estos archivos terminó antes de resolver su fusión. Sus elecciones y ediciones pueden recuperarse.
prompt-restore = Restaurar
prompt-start-over = Empezar de nuevo
prompt-cannot-restore = No se puede restaurar la sesión
prompt-save-changes = ¿Guardar los cambios?
prompt-cannot-save = No se puede guardar el archivo
prompt-ok = Aceptar
prompt-changed-detail = Recárgalo y pierde tus ediciones, o conserva tu versión y sobrescríbelo más tarde.
prompt-reload = Recargar
prompt-keep-mine = Conservar la mía
prompt-changed-on-disk = { $name } cambió en el disco.
prompt-descendants-changed =
    { $count ->
        [one] Guardar cambia los conflictos de { $count } descendiente.
       *[other] Guardar cambia los conflictos de { $count } descendientes.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } conflicto conserva sus marcadores, y git o jj ven la fusión sin resolver.
       *[other] { $count } conflictos conservan sus marcadores, y git o jj ven la fusión sin resolver.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } conflicto conserva sus marcadores, y git o jj ven la fusión sin resolver. Después, git mergetool restaura su propia copia del archivo; ejecútalo de nuevo para restaurar esta sesión.
       *[other] { $count } conflictos conservan sus marcadores, y git o jj ven la fusión sin resolver. Después, git mergetool restaura su propia copia del archivo; ejecútalo de nuevo para restaurar esta sesión.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } archivo
       *[other] { $count } archivos
    }
count-versions =
    { $count ->
        [one] { $count } versión
       *[other] { $count } versiones
    }
count-hunks =
    { $count ->
        [one] { $count } fragmento
       *[other] { $count } fragmentos
    }
count-differences =
    { $count ->
        [one] { $count } diferencia
       *[other] { $count } diferencias
    }
count-conflicts =
    { $count ->
        [one] { $count } conflicto
       *[other] { $count } conflictos
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } línea sin cambios
       *[other] { $count } líneas sin cambios
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } bloque duplicado
       *[other] { $count } bloques duplicados
    }

## Conflicted commits (stack window)

no-paths = ninguno
stack-title = Commits con conflictos
stack-empty = No hay commits con conflictos.
refresh = Actualizar
stack-preview = Vista previa del guardado
preview-running = Calculando qué haría el guardado…
working-copy = copia de trabajo
no-description = (sin descripción)
commit-tooltip =
    ID de cambio: { $change }
    ID de commit: { $commit }
    Fecha: { $date }
inherited = (heredado)
stack-resolved = { $path } resuelto en { $change }.
stack-saved-conflicts = { $path } guardado en { $change } con conflictos.
stack-error = La fusión de { $path } en { $change } terminó con un error.
preview-resolves = Guardar resuelve la ruta en { $change }.
preview-keeps = Guardar mantiene conflictos en { $change }.
preview-no-rebase = No se rebasa ningún descendiente.

## Status icons

status-added = Añadido
status-modified = Modificado
status-deleted = Eliminado
status-moved = Movido
status-copied = Copiado
status-conflicted = En conflicto

## Changed files (directory window)

dirs-title = Archivos modificados
dirs-empty = Ningún archivo difiere.
dirs-against = { $files } respecto a { $rev }
file-added = añadido
file-deleted = eliminado
file-modified = modificado
file-renamed = renombrado desde { $from }
file-copied = copiado desde { $from }
file-from = desde { $from }
file-undecodable = no decodificable
file-binary = binario
file-special = especial
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ en la operación { $operation }
no-file = { $name } (sin archivo)

## History window

history-title = Historial de { $path }
history-empty = No hay versiones.
version-conflict = (conflicto)
version-absent = (ausente)
version-other = (otro)

## Absorb window

absorb-title = Absorber
absorb-empty = Nada que absorber.
absorb-apply = Aplicar
absorb-keep = Copia de trabajo
absorb-note = Se absorbieron { $hunks } en { $commits }.
absorb-stale = La copia de trabajo está desactualizada: ejecuta jj workspace update-stale.
absorb-skipped = Se omitió { $path }: { $reason }

## Open dialog

open-title-merge = Abrir una fusión
open-title-diff = Abrir una comparación
mode-merge = Fusión
mode-diff = Comparación
start-merge = Fusionar
start-diff = Comparar
slot-base = Base
slot-left = Izquierda
slot-right = Derecha
slot-output = Salida
choose-base = Elegir el archivo base
choose-left = Elegir el archivo izquierdo
choose-right = Elegir el archivo derecho
choose-output = Elegir el archivo de salida
missing-slot = Elige un archivo para { $slot }.

## Menus

menu-settings = Ajustes…
menu-save = Guardar
menu-save-and-exit = Guardar y salir
menu-save-copy-as = Guardar copia como…
menu-cancel = Cancelar
menu-edit = Edición
menu-merge = Fusión
menu-go = Ir
menu-view = Ver
menu-undo = Deshacer
menu-redo = Rehacer
menu-find = Buscar…
menu-go-to-line = Ir a la línea…
menu-toggle-left = Alternar izquierda
menu-toggle-base = Alternar base
menu-toggle-right = Alternar derecha
menu-clear-choice = Borrar elección
menu-take-base = Tomar base
menu-take-left = Tomar izquierda
menu-take-right = Tomar derecha
menu-take-left-then-right = Tomar izquierda y luego derecha
menu-take-right-then-left = Tomar derecha y luego izquierda
menu-take-sorted-union = Tomar unión ordenada
menu-take-left-all-unresolved = Tomar izquierda en todos los no resueltos
menu-take-right-all-unresolved = Tomar derecha en todos los no resueltos
menu-take-whole-left = Tomar el archivo izquierdo completo
menu-take-whole-right = Tomar el archivo derecho completo
menu-carry-moved-edit = Llevar la edición al bloque movido
menu-drop-duplicate = Quitar duplicado
menu-next-difference = Diferencia siguiente
menu-previous-difference = Diferencia anterior
menu-next-conflict = Conflicto siguiente
menu-previous-conflict = Conflicto anterior
menu-go-to-move-end = Ir al otro extremo del movimiento
menu-base-pane = Panel base
menu-blame = Autoría
menu-syntax = Resaltado de sintaxis
menu-soft-wrap = Ajuste de línea
menu-whitespace = Mostrar espacios en blanco
menu-collapse-unchanged = Contraer sin cambios
menu-scroll-sync = Sincronizar desplazamiento del resultado

## Toolbar

tool-label-difference = Diferencia
tool-label-conflict = Conflicto
tool-label-left = Izquierda
tool-label-right = Derecha
more = Más
more-menu = Más elecciones
more-base = Base
more-left-then-right = Izquierda y luego derecha
more-right-then-left = Derecha y luego izquierda
more-none = Ninguna
tool-previous-difference = Diferencia anterior
tool-next-difference = Diferencia siguiente
tool-previous-conflict = Conflicto anterior
tool-next-conflict = Conflicto siguiente
tool-left = Alternar izquierda en la región actual
tool-base = Alternar base en la región actual
tool-right = Alternar derecha en la región actual
tool-left-then-right = Tomar izquierda y luego derecha en la región actual
tool-right-then-left = Tomar derecha y luego izquierda en la región actual
tool-clear = Borrar la elección de la región actual
tool-undo = Deshacer
tool-redo = Rehacer
tool-base-pane = Mostrar u ocultar el panel base
tool-find = Buscar
tool-go-to-line = Ir a una línea del resultado
tool-syntax = Activar o desactivar los colores de sintaxis
tool-soft-wrap = Activar o desactivar el ajuste de línea
tool-whitespace = Mostrar u ocultar espacios en blanco

## Conflict badge and pick targets

pick-label = { $title } en la región { $region } de { $regions }
pick-label-unresolved = { $title } en la región { $region } de { $regions }, sin resolver
badge-conflict = Conflicto { $number } de { $count }
badge-sides = { $kept } de { $sides } lados
badge-all-resolved = todos resueltos
badge-resolved-one = Conflicto resuelto
badge-resolved-all = Los { $count } resueltos
badge-unresolved-of = { $unresolved } de { $count } sin resolver

## Panes

role-left = Izquierda
role-right = Derecha
role-base = Base
role-side = Lado { $number }
role-base-of = Base { $number }
role-result = Resultado
cell-line = { $title }, línea { $line }: { $text }
cell-no-line = { $title }, sin línea
cell-no-line-deleted = { $title }, sin línea: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Nada elegido ({ $count } línea descartada)
       *[other] Nada elegido ({ $count } líneas descartadas)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } línea eliminada ({ $roles })
       *[other] { $count } líneas eliminadas ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } línea eliminada
       *[other] { $count } líneas eliminadas
    }
previous-match = Coincidencia anterior en { $title }
next-match = Coincidencia siguiente en { $title }
match-count = { $number } de { $count }
go-to-line-prompt = Ir a la línea (1–{ $last }):
go-to-line-current = Línea actual { $line } de { $last }
go-to-line-target = Ir a la línea { $line }
jj-diff-banner = Son copias temporales de jj diff --tool. Las ediciones se pierden al cerrar la ventana.
copy-to-right = Copiar esta diferencia a la derecha
copy-to-left = Copiar esta diferencia a la izquierda
initial-base = B
initial-left = I
initial-right = D
moved-to-base = Bloque movido: ir a su otro extremo, línea { $line } de la base
moved-to-left = Bloque movido: ir a su otro extremo, línea { $line } de la izquierda
moved-to-right = Bloque movido: ir a su otro extremo, línea { $line } de la derecha
diff-pane-left = Izquierda: { $name }
diff-pane-right = Derecha: { $name }

## Status bar

repo-git = repo git
repo-jj = repo jj
repo-none = sin repo
mode-git = fusión git
mode-jj = fusión jj
mode-git-word = fusión git por palabra
mode-jj-word = fusión jj por palabra
encoding-utf8-bom = UTF-8 con BOM
inputs-differ = { $encoding } (las entradas difieren)
word-merged = { $count } fusionados por palabra
unresolved = { $count } sin resolver
mixed-endings = finales mixtos
compare-whitespace = espacios significativos
compare-line-endings = finales de línea ignorados
compare-trailing = espacios finales ignorados
compare-amount = cantidad de espacios ignorada
compare-all = todos los espacios ignorados
case-ignored = mayúsculas ignoradas
patterns-ignored = patrones ignorados
blank-lines-ignored = líneas vacías ignoradas
diverge-whitespace-jj = espacios comparados distinto que jj
diverge-case-git = mayúsculas ignoradas, a diferencia de git
diverge-case-jj = mayúsculas ignoradas, a diferencia de jj
diverge-patterns-git = patrones ignorados, a diferencia de git
diverge-patterns-jj = patrones ignorados, a diferencia de jj
diverge-same-change-git = cambios iguales conservados, a diferencia de git
diverge-word-git = palabras fusionadas, a diferencia de git
diverge-deep-git = conflictos divididos por dentro, a diferencia de git
diverge-deep-jj = conflictos divididos por dentro, a diferencia de jj
diverge-syntax-git = límites de sintaxis respetados, a diferencia de git
diverge-syntax-jj = límites de sintaxis respetados, a diferencia de jj
copy-saved =
    { $count ->
        [0] Copia guardada, resuelta
        [one] Copia guardada con { $count } conflicto
       *[other] Copia guardada con { $count } conflictos
    }

## Result pane's right-click menu

choice-none = Ninguna
choice-base = Base
choice-left = Izquierda
choice-right = Derecha
choice-left-then-right = Izquierda + derecha
choice-right-then-left = Derecha + izquierda
action-sorted-union = Unión ordenada
suggested = Sugerido: { $action }
suggested-carry = Sugerido: llevar la edición de { $from } a la línea { $line }
suggested-drop = Sugerido: quitar el duplicado de la línea { $line }
edit-cut = Cortar
edit-copy = Copiar
edit-paste = Pegar
edit-select-all = Seleccionar todo
