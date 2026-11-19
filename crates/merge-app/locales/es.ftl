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
prompt-output-changed-detail = Otro programa lo cambió desde que se abrió la fusión. Guardar y salir reemplaza esos cambios por la fusión.
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
stack-inherited = { $path } en { $change } hereda su conflicto de { $origin }. Resolverlo primero en { $origin } lo elimina aquí; una resolución guardada aquí primero probablemente volverá a entrar en conflicto.
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
menu-copy-as-patch = Copiar como parche
menu-copy-difference-as-patch = Copiar diferencia como parche
menu-toggle-left = Alternar izquierda
menu-toggle-base = Alternar base
menu-toggle-right = Alternar derecha
menu-clear-choice = Borrar elección
menu-pick-selected-lines = Elegir líneas seleccionadas
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
menu-unified = Unificada
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
pick-label-inherited = { $title } en la región { $region } de { $regions }, sin resolver, heredado
inherited-pill = heredado
line-pick-label = Línea { $line } de { $title }
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
snapshot-failed-banner = Guardado, pero jj no registró el guardado: { $error }
output-changed-banner = { $name } cambió en el disco desde que se abrió la fusión. Guardar y salir lo reemplaza.
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
diff-pane-unified = Unificada: { $left } frente a { $right }

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
saved =
    { $count ->
        [0] Guardado, resuelto
        [one] Guardado con { $count } conflicto
       *[other] Guardado con { $count } conflictos
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

## Error details

err-cannot-read = no se puede leer { $path }: { $detail }
err-cannot-read-plain = no se puede leer { $path }
err-cannot-write = no se puede escribir { $path }: { $detail }
err-cannot-save = no se puede guardar { $path }: { $detail }
err-cannot-find = no se encuentra { $path }: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $path } en { $rev }: { $detail }
err-not-recorded = { $path } está escrito, pero el repositorio no registró el guardado: { $detail }
err-save-writing = un guardado está escribiendo la fusión
err-no-output = la fusión no tiene salida que escribir
err-save-running = hay un guardado en curso
err-encode = { $encoding } no puede contener { $character } (U+{ $code }) en la línea { $line }
err-binary = archivo binario: un byte NUL en los primeros { $bytes } bytes
err-invalid-utf8 = UTF-8 no válido en el byte { $offset }
err-invalid-utf16 = sustituto UTF-16 sin pareja en el byte { $offset }
err-odd-utf16 = archivo UTF-16 con un número impar de bytes
err-not-a-repository = { $path } no está dentro de un repositorio git ni jj
err-not-utf8-path = { $path } no es una ruta UTF-8
err-no-working-copy = { $path } no tiene copia de trabajo
err-outside-repository = { $path } está fuera del repositorio
err-stale-file = { $path } en disco no es el del commit de la copia de trabajo; ejecute jj workspace update-stale y vuelva a intentarlo
err-stale-workspace = la copia de trabajo en { $path } está desactualizada; ejecute jj workspace update-stale y vuelva a intentarlo
err-unknown-revision = revisión desconocida { $rev }
err-ambiguous-revision = la revisión { $rev } es ambigua
err-conflicted-at = { $path } tiene conflictos en { $rev }
err-no-index = { $name } no tiene índice
err-immutable = el commit { $rev } es inmutable
err-not-conflicted-at = { $path } no tiene conflictos en { $rev }
err-bad-config = { $key } tiene el valor desconocido { $value }
err-backend = { $context }: { $detail }
err-concurrent-operations = el repositorio tiene { $count } operaciones concurrentes; cualquier comando jj las reconcilia
err-not-a-file-conflict = { $path } no es un conflicto de archivos
err-unknown-operation = no existe la operación { $op }: { $reason }
err-no-working-copy-at = el espacio de trabajo no tiene copia de trabajo en la operación { $op }
err-no-hunk = no existe el bloque { $hunk }; hay { $count }
err-not-a-destination = { $rev } no es un ancestro mutable de la copia de trabajo en --into
err-repository-moved = el repositorio cambió desde la operación { $op }; vuelva a listar los bloques
err-conflict-changed = { $path } en { $change } cambió desde que triad lo leyó; no se escribe nada
err-sides-changed = { $path }: jj releyó un conflicto de { $after } lados en lugar de uno de { $before }
err-jj-config-load = no se puede leer la configuración de jj { $path }: { $detail }
err-jj-config-id = { $path }: no es un id de configuración de jj
err-jj-config-value = { $key } en la configuración de jj: { $detail }
err-jj-config-value-in = { $path }: { $key } en la configuración de jj: { $detail }
err-jj-config-get = al leer un valor de configuración de jj
err-jj-config-set = al establecer un valor de configuración de jj
err-jj-config-find = al buscar la configuración de jj del repositorio o del espacio de trabajo
err-auto-track = `snapshot.auto-track = { $text }` no es un fileset
err-git-ignores = al leer los archivos de exclusión de git
err-jj-workspace = al cargar el espacio de trabajo de jj
err-needs-jj = { $command } necesita un repositorio jj
err-not-conflicted = { $path } no tiene conflictos
err-term-not-text = { $path }: { $term } no es texto: { $detail }
err-term-not-a-file = { $path }: { $term } es un { $kind }; solo se pueden resolver conflictos de texto
err-kind-symlink = enlace simbólico
err-kind-submodule = submódulo
err-kind-directory = directorio
err-deleted-everywhere = { $path } está eliminado en todos los lados; no hay nada que fusionar
err-resolved-in-tree = { $path } ya no tiene marcadores de conflicto en el árbol de trabajo, así que parece resuelto; márquelo como resuelto o restaure sus marcadores para fusionarlo de nuevo
err-tree-not-text = { $path }: el archivo del árbol de trabajo no es texto ({ $detail }), así que puede contener ediciones que un guardado sobrescribiría; restaure el texto con sus marcadores de conflicto o resuélvalo a mano
err-temporary = no se puede crear un archivo temporal para el resultado: { $detail }
err-not-a-directory = { $path } no es un directorio
err-mixed = diff toma dos archivos o dos directorios: { $dir } es un directorio, { $other } no
err-git-in-jj = { $path } es un repositorio jj, donde triad fusiona solo con la semántica de jj; resuelva con `jj resolve`
err-git-in-jj-resolve = { $path } es el directorio de resolución de jj, donde triad fusiona solo con la semántica de jj; resuelva con `jj resolve`
err-no-markers = no hay marcadores de conflicto
err-unclosed = línea { $line }: el conflicto no está cerrado
err-nested = línea { $line }: un conflicto empieza dentro de otro
err-out-of-order = línea { $line }: el marcador está fuera de orden
err-no-separator = línea { $line }: el conflicto termina antes de su =======
err-bad-diff-line = línea { $line }: una línea de diff no empieza ni por ' ', ni por '-' ni por '+'
err-marker-shape = línea { $line }: un conflicto necesita al menos dos lados y una base menos, hay { $sides } lados y { $bases } bases
err-mixed-arity = línea { $line }: un conflicto de { $found } lados tras conflictos de { $expected }
err-shape = una fusión necesita al menos dos lados y una base menos que lados, hay { $sides } lados y { $bases } bases
err-journal-header = el diario no tiene una línea de cabecera completa
err-journal-line = la línea { $index } del diario no se puede analizar: { $detail }
err-journal-version = el diario tiene el formato { $found }, no { $expected }
err-journal-inputs = el diario se escribió para otras entradas
err-journal-alignment = el diario se escribió para otra alineación de estas entradas, como una de otra versión de triad
err-journal-command = el comando { $index } del diario no se aplica: { $detail }
err-cannot-open-window = no se puede abrir la ventana: { $detail }
err-io-not-found = no existe el archivo o directorio
err-io-permission = permiso denegado
err-io-exists = el archivo ya existe
err-io-is-directory = es un directorio
err-io-not-directory = no es un directorio
err-io-full = no queda espacio en el dispositivo
err-io-read-only = el sistema de archivos es de solo lectura

## gpui-component's text

component-replace = Reemplazar
component-replace-all = Reemplazar todo
component-clear = Borrar
