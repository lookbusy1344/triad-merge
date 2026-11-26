# The windows' text in Russian. `en.ftl` is the reference: it says how counts and names work.
# Russian counts take three forms: `[one]` (1, 21, 31…), `[few]` (2–4, 22–24…) and `[many]`
# (0, 5–20, 25–30…). `[many]` is the default.

## About window

about-title = О программе triad
about-description = Трёхстороннее слияние и двустороннее сравнение
about-vcs-support = Создано для Jujutsu, с полной поддержкой Git
about-version = Версия { $version }
about-build-commit = Коммит { $commit }
about-build-commit-change = Коммит { $commit }, изменение { $change }

## Window parts

result-title = Результат
merge-toolbar = Инструменты слияния
diff-toolbar = Инструменты сравнения
input-lines = Входные строки
go-to-line-field = Номер строки
no-matches = Нет совпадений
close = Закрыть
settings-errors = В файле настроек есть ошибки: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Сохранить с неразрешёнными конфликтами?
prompt-save-with-conflicts = Сохранить с конфликтами
prompt-keep-editing = Продолжить правку
prompt-discard-detail = «Отбросить» ничего не записывает, и слияние остаётся неразрешённым в git или jj.
prompt-discard = Отбросить
prompt-cannot-write = Не удаётся записать результат
prompt-cannot-write-copy = Не удаётся записать копию
prompt-cannot-record = Репозиторий не записал сохранение
prompt-no-output = У этого окна нет выходного файла.
prompt-quit = Выйти
prompt-try-again = Повторить
prompt-restore-session = Восстановить предыдущий сеанс?
prompt-restore-detail = Сеанс работы с этими файлами завершился до разрешения слияния. Его выбор и правки можно вернуть.
prompt-restore = Восстановить
prompt-start-over = Начать заново
prompt-cannot-restore = Не удаётся восстановить сеанс
prompt-save-changes = Сохранить изменения?
prompt-cannot-save = Не удаётся сохранить файл
prompt-ok = ОК
prompt-changed-detail = Перезагрузите файл и потеряйте правки или оставьте свою версию и сохраните её поверх позже.
prompt-reload = Перезагрузить
prompt-output-changed-detail = Другая программа изменила его после открытия слияния. «Сохранить и выйти» заменит эти изменения результатом слияния.
prompt-keep-mine = Оставить мою
prompt-changed-on-disk = Файл { $name } изменился на диске.
prompt-descendants-changed =
    { $count ->
        [one] Сохранение меняет конфликты { $count } потомка.
       *[many] Сохранение меняет конфликты { $count } потомков.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } конфликт сохраняет свои маркеры, и git или jj считают слияние неразрешённым.
        [few] { $count } конфликта сохраняют свои маркеры, и git или jj считают слияние неразрешённым.
       *[many] { $count } конфликтов сохраняют свои маркеры, и git или jj считают слияние неразрешённым.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } конфликт сохраняет свои маркеры, и git или jj считают слияние неразрешённым. Затем git mergetool вернёт свою копию файла; запустите его снова, чтобы восстановить этот сеанс.
        [few] { $count } конфликта сохраняют свои маркеры, и git или jj считают слияние неразрешённым. Затем git mergetool вернёт свою копию файла; запустите его снова, чтобы восстановить этот сеанс.
       *[many] { $count } конфликтов сохраняют свои маркеры, и git или jj считают слияние неразрешённым. Затем git mergetool вернёт свою копию файла; запустите его снова, чтобы восстановить этот сеанс.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } коммит
        [few] { $count } коммита
       *[many] { $count } коммитов
    }
count-files =
    { $count ->
        [one] { $count } файл
        [few] { $count } файла
       *[many] { $count } файлов
    }
count-versions =
    { $count ->
        [one] { $count } версия
        [few] { $count } версии
       *[many] { $count } версий
    }
count-hunks =
    { $count ->
        [one] { $count } фрагмент
        [few] { $count } фрагмента
       *[many] { $count } фрагментов
    }
count-differences =
    { $count ->
        [one] { $count } различие
        [few] { $count } различия
       *[many] { $count } различий
    }
count-conflicts =
    { $count ->
        [one] { $count } конфликт
        [few] { $count } конфликта
       *[many] { $count } конфликтов
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } неизменённая строка
        [few] { $count } неизменённые строки
       *[many] { $count } неизменённых строк
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } повторённый блок
        [few] { $count } повторённых блока
       *[many] { $count } повторённых блоков
    }

## Conflicted commits (stack window)

no-paths = нет
stack-title = Коммиты с конфликтами
stack-empty = Нет коммитов с конфликтами.
refresh = Обновить
stack-preview = Предпросмотр сохранения
preview-running = Вычисляется результат сохранения…
working-copy = рабочая копия
no-description = (описание не задано)
commit-tooltip =
    ID изменения: { $change }
    ID коммита: { $commit }
    Дата: { $date }
inherited = (унаследован)
stack-resolved = { $path } разрешён в { $change }.
stack-saved-conflicts = { $path } сохранён в { $change } с конфликтами.
stack-error = Слияние { $path } в { $change } завершилось ошибкой.
stack-inherited = { $path } в { $change } наследует конфликт от { $origin }. Если сначала разрешить его в { $origin }, он исчезнет и здесь; разрешение, сохранённое сначала здесь, скорее всего, снова вызовет конфликт.
preview-resolves = Сохранение разрешит путь в { $change }.
preview-keeps = После сохранения в { $change } останутся конфликты.
preview-no-rebase = Ни один потомок не перебазируется.

## Status icons

status-added = Добавлен
status-modified = Изменён
status-deleted = Удалён
status-moved = Перемещён
status-copied = Скопирован
status-conflicted = Конфликт

## Changed files (directory window)

dirs-title = Изменённые файлы
dirs-empty = Различающихся файлов нет.
dirs-against = { $files } относительно { $rev }
file-added = добавлен
file-deleted = удалён
file-modified = изменён
file-renamed = переименован из { $from }
file-copied = скопирован из { $from }
file-from = из { $from }
file-undecodable = не декодируется
file-binary = двоичный
file-special = особый
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ на операции { $operation }
no-file = { $name } (нет файла)

## History window

history-title = История { $path }
history-empty = Нет версий.
version-conflict = (конфликт)
evolog-title = Версии { $change }
interdiff-title = { $change } с { $short }
dirs-since = { $files } с { $short }
interdiff-rebased = { $change } на { $short }, перебазировано
interdiff-version = { $change } на { $short }
evolog-no-difference = Нет отличий от текущей версии.
version-absent = (отсутствует)
version-other = (другое)

## Absorb window

absorb-title = Поглощение
absorb-empty = Нечего поглощать.
absorb-apply = Применить
absorb-keep = Рабочая копия
absorb-note = Поглощено: { $hunks } в { $commits }.
absorb-stale = Рабочая копия устарела: выполните jj workspace update-stale.
absorb-skipped = Пропущен { $path }: { $reason }

## Open dialog

open-title-merge = Открыть слияние
open-title-diff = Открыть сравнение
mode-merge = Слияние
mode-diff = Сравнение
start-merge = Слить
start-diff = Сравнить
slot-base = База
slot-left = Слева
slot-right = Справа
slot-output = Результат
choose-base = Выбрать базовый файл
choose-left = Выбрать левый файл
choose-right = Выбрать правый файл
choose-output = Выбрать выходной файл
missing-slot = Выберите файл для «{ $slot }».

## Menus

menu-settings = Настройки…
menu-save = Сохранить
menu-save-and-exit = Сохранить и выйти
menu-save-copy-as = Сохранить копию как…
menu-cancel = Отмена
menu-edit = Правка
menu-merge = Слияние
menu-go = Переход
menu-view = Вид
menu-undo = Отменить
menu-redo = Повторить
menu-find = Найти…
menu-go-to-line = Перейти к строке…
menu-copy-as-patch = Скопировать как патч
menu-copy-difference-as-patch = Скопировать различие как патч
menu-toggle-left = Переключить левую
menu-toggle-base = Переключить базу
menu-toggle-right = Переключить правую
menu-clear-choice = Сбросить выбор
menu-pick-selected-lines = Выбрать выделенные строки
menu-take-base = Взять базу
menu-take-left = Взять левую
menu-take-right = Взять правую
menu-take-left-then-right = Взять левую, затем правую
menu-take-right-then-left = Взять правую, затем левую
menu-take-sorted-union = Взять отсортированное объединение
menu-take-left-all-unresolved = Взять левую во всех неразрешённых
menu-take-right-all-unresolved = Взять правую во всех неразрешённых
menu-take-whole-left = Взять весь левый файл
menu-take-whole-right = Взять весь правый файл
menu-carry-moved-edit = Перенести правку в перемещённый блок
menu-drop-duplicate = Удалить повтор
menu-next-difference = Следующее различие
menu-previous-difference = Предыдущее различие
menu-next-conflict = Следующий конфликт
menu-previous-conflict = Предыдущий конфликт
menu-go-to-move-end = К другому концу перемещения
menu-base-pane = Панель базы
menu-blame = Авторство
menu-syntax = Подсветка синтаксиса
menu-soft-wrap = Перенос строк
menu-whitespace = Показывать пробелы
menu-collapse-unchanged = Свернуть неизменённое
menu-unified = Единый вид
menu-scroll-sync = Синхронная прокрутка результата

## Toolbar

tool-label-difference = Различие
tool-label-conflict = Конфликт
tool-label-left = Слева
tool-label-right = Справа
more = Ещё
more-menu = Другие варианты
more-base = База
more-left-then-right = Слева, затем справа
more-right-then-left = Справа, затем слева
more-none = Ничего
tool-previous-difference = Предыдущее различие
tool-next-difference = Следующее различие
tool-previous-conflict = Предыдущий конфликт
tool-next-conflict = Следующий конфликт
tool-left = Переключить левую в текущей области
tool-base = Переключить базу в текущей области
tool-right = Переключить правую в текущей области
tool-left-then-right = Взять левую, затем правую в текущей области
tool-right-then-left = Взять правую, затем левую в текущей области
tool-clear = Сбросить выбор текущей области
tool-undo = Отменить
tool-redo = Повторить
tool-base-pane = Показать или скрыть панель базы
tool-find = Найти
tool-go-to-line = Перейти к строке результата
tool-syntax = Включить или выключить цвета синтаксиса
tool-soft-wrap = Включить или выключить перенос строк
tool-whitespace = Показать или скрыть пробелы

## Conflict badge and pick targets

pick-label = { $title } в области { $region } из { $regions }
pick-label-unresolved = { $title } в области { $region } из { $regions }, не разрешено
pick-label-inherited = { $title } в области { $region } из { $regions }, не разрешено, унаследовано
inherited-pill = унаследован
line-pick-label = Строка { $line } в { $title }
badge-conflict = Конфликт { $number } из { $count }
badge-sides = { $kept } из { $sides } сторон
badge-all-resolved = все разрешены
badge-resolved-one = Конфликт разрешён
badge-resolved-all = Все { $count } разрешены
badge-unresolved-of = Не разрешено: { $unresolved } из { $count }

## Panes

role-left = Слева
role-right = Справа
role-base = База
role-side = Сторона { $number }
role-base-of = База { $number }
role-result = Результат
cell-line = { $title }, строка { $line }: { $text }
cell-no-line = { $title }, нет строки
cell-no-line-deleted = { $title }, нет строки: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Ничего не выбрано (отброшена { $count } строка)
        [few] Ничего не выбрано (отброшено { $count } строки)
       *[many] Ничего не выбрано (отброшено { $count } строк)
    }
placeholder-deleted =
    { $count ->
        [one] Удалена { $count } строка ({ $roles })
        [few] Удалено { $count } строки ({ $roles })
       *[many] Удалено { $count } строк ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] Удалена { $count } строка
        [few] Удалено { $count } строки
       *[many] Удалено { $count } строк
    }
previous-match = Предыдущее совпадение в «{ $title }»
next-match = Следующее совпадение в «{ $title }»
match-count = { $number } из { $count }
go-to-line-prompt = Перейти к строке (1–{ $last }):
go-to-line-current = Текущая строка { $line } из { $last }
go-to-line-target = Перейти к строке { $line }
jj-diff-banner = Это временные копии от jj diff --tool. Правки здесь пропадут при закрытии окна.
snapshot-failed-banner = Сохранено, но jj не записал сохранение: { $error }
output-changed-banner = { $name } изменён на диске после открытия слияния. «Сохранить и выйти» заменит его.
copy-to-right = Скопировать это различие вправо
copy-to-left = Скопировать это различие влево
initial-base = Б
initial-left = Л
initial-right = П
moved-to-base = Перемещённый блок: перейти к другому концу, строка { $line } базы
moved-to-left = Перемещённый блок: перейти к другому концу, строка { $line } слева
moved-to-right = Перемещённый блок: перейти к другому концу, строка { $line } справа
diff-pane-left = Слева: { $name }
diff-pane-right = Справа: { $name }
diff-pane-unified = Единый вид: { $left } против { $right }

## Status bar

repo-git = репозиторий git
repo-jj = репозиторий jj
repo-none = без репозитория
mode-git = слияние git
mode-jj = слияние jj
mode-git-word = слияние git по словам
mode-jj-word = слияние jj по словам
encoding-utf8-bom = UTF-8 с BOM
encoding-utf16le-bom = UTF-16 LE с BOM
encoding-utf16be-bom = UTF-16 BE с BOM
encoding-utf8-as-windows-1252 = UTF-8, прочитанный как Windows-1252
inputs-differ = { $encoding } (входы различаются)
word-merged = Слито по словам: { $count }
unresolved = Не разрешено: { $count }
mixed-endings = смешанные концы строк
compare-whitespace = пробелы значимы
compare-line-endings = концы строк игнорируются
compare-trailing = пробелы в конце строк игнорируются
compare-amount = количество пробелов игнорируется
compare-all = все пробелы игнорируются
case-ignored = регистр игнорируется
patterns-ignored = шаблоны игнорируются
blank-lines-ignored = пустые строки игнорируются
diverge-whitespace-jj = пробелы сравниваются не как в jj
diverge-case-git = регистр игнорируется, в отличие от git
diverge-case-jj = регистр игнорируется, в отличие от jj
diverge-patterns-git = шаблоны игнорируются, в отличие от git
diverge-patterns-jj = шаблоны игнорируются, в отличие от jj
diverge-same-change-git = одинаковые изменения сохранены, в отличие от git
diverge-word-git = слова слиты, в отличие от git
diverge-deep-git = конфликты разделены внутри, в отличие от git
diverge-deep-jj = конфликты разделены внутри, в отличие от jj
diverge-syntax-git = границы синтаксиса соблюдены, в отличие от git
diverge-syntax-jj = границы синтаксиса соблюдены, в отличие от jj
copy-saved =
    { $count ->
        [0] Копия сохранена, всё разрешено
        [one] Копия сохранена с { $count } конфликтом
       *[many] Копия сохранена с { $count } конфликтами
    }
saved =
    { $count ->
        [0] Сохранено, всё разрешено
        [one] Сохранено с { $count } конфликтом
       *[many] Сохранено с { $count } конфликтами
    }

## Result pane's right-click menu

choice-none = Ничего
choice-base = База
choice-left = Слева
choice-right = Справа
choice-left-then-right = Слева + справа
choice-right-then-left = Справа + слева
action-sorted-union = Отсортированное объединение
suggested = Предложение: { $action }
suggested-carry = Предложение: перенести правку «{ $from }» в строку { $line }
suggested-drop = Предложение: удалить повтор строки { $line }
edit-cut = Вырезать
edit-copy = Копировать
edit-paste = Вставить
edit-select-all = Выбрать всё

## Error details

err-cannot-read = не удаётся прочитать { $path }: { $detail }
err-cannot-read-plain = не удаётся прочитать { $path }
err-cannot-write = не удаётся записать { $path }: { $detail }
err-cannot-save = не удаётся сохранить { $path }: { $detail }
err-cannot-find = не удаётся найти { $path }: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $path } в { $rev }: { $detail }
err-not-recorded = { $path } записан, но репозиторий не зафиксировал сохранение: { $detail }
err-save-writing = идёт запись слияния
err-no-output = у слияния нет вывода для записи
err-save-running = идёт сохранение
err-encode = { $encoding } не может содержать { $character } (U+{ $code }) в строке { $line }
err-binary = двоичный файл: байт NUL в первых { $bytes } байтах
err-invalid-utf8 = недопустимый UTF-8 в байте { $offset }
err-invalid-utf16 = непарный суррогат UTF-16 в байте { $offset }
err-odd-utf16 = файл UTF-16 с нечётным числом байтов
err-not-a-repository = { $path } не находится в репозитории git или jj
err-not-utf8-path = { $path } — не путь UTF-8
err-no-working-copy = у { $path } нет рабочей копии
err-outside-repository = { $path } находится вне репозитория
err-stale-file = { $path } на диске не совпадает с коммитом рабочей копии; выполните jj workspace update-stale и повторите
err-stale-workspace = рабочая копия в { $path } устарела; выполните jj workspace update-stale и повторите
err-unknown-revision = неизвестная ревизия { $rev }
err-ambiguous-revision = ревизия { $rev } неоднозначна
err-conflicted-at = в { $path } есть конфликты в { $rev }
err-no-index = у { $name } нет индекса
err-immutable = коммит { $rev } неизменяем
err-not-conflicted-at = в { $path } нет конфликтов в { $rev }
err-bad-config = у { $key } неизвестное значение { $value }
err-backend = { $context }: { $detail }
err-concurrent-operations = в репозитории есть параллельные операции ({ $count }); любая команда jj их согласует
err-not-a-file-conflict = { $path } — не конфликт файлов
err-unknown-operation = нет операции { $op }: { $reason }
err-no-working-copy-at = у рабочей области нет рабочей копии в операции { $op }
err-no-hunk = нет фрагмента { $hunk }; всего фрагментов: { $count }
err-not-a-destination = { $rev } — не изменяемый предок рабочей копии в --into
err-repository-moved = репозиторий изменился после операции { $op }; получите список фрагментов заново
err-conflict-changed = { $path } в { $change } изменился после чтения triad; ничего не записано
err-sides-changed = { $path }: jj прочитал конфликт с числом сторон { $after } вместо { $before }
err-jj-config-load = не удаётся прочитать конфигурацию jj { $path }: { $detail }
err-jj-config-id = { $path }: не идентификатор конфигурации jj
err-jj-config-value = { $key } в конфигурации jj: { $detail }
err-jj-config-value-in = { $path }: { $key } в конфигурации jj: { $detail }
err-jj-config-get = чтение значения конфигурации jj
err-jj-config-set = установка значения конфигурации jj
err-jj-config-find = поиск конфигурации jj репозитория или рабочей области
err-auto-track = `snapshot.auto-track = { $text }` — не набор файлов (fileset)
err-git-ignores = чтение файлов игнорирования git
err-jj-workspace = загрузка рабочей области jj
err-needs-jj = { $command } требует репозиторий jj
err-not-conflicted = в { $path } нет конфликтов
err-term-not-text = { $path }: { $term } — не текст: { $detail }
err-term-not-a-file = { $path }: { $term } — { $kind }; разрешать можно только текстовые конфликты
err-kind-symlink = символическая ссылка
err-kind-submodule = подмодуль
err-kind-directory = каталог
err-deleted-everywhere = { $path } удалён на всех сторонах; сливать нечего
err-resolved-in-tree = в { $path } не осталось маркеров конфликта в рабочем дереве, поэтому он выглядит разрешённым; отметьте его как разрешённый или верните маркеры, чтобы слить заново
err-tree-not-text = { $path }: файл в рабочем дереве — не текст ({ $detail }), поэтому в нём могут быть правки, которые сохранение перезапишет; верните текст с маркерами конфликта или разрешите вручную
err-not-a-directory = { $path } — не каталог
err-mixed = diff принимает два файла или два каталога: { $dir } — каталог, { $other } — нет
err-git-in-jj = { $path } — репозиторий jj, где triad сливает только по семантике jj; разрешайте через `jj resolve`
err-git-in-jj-resolve = { $path } — каталог разрешения jj, где triad сливает только по семантике jj; разрешайте через `jj resolve`
err-no-markers = нет маркеров конфликта
err-unclosed = строка { $line }: конфликт не закрыт
err-nested = строка { $line }: конфликт начинается внутри другого
err-out-of-order = строка { $line }: маркер стоит не по порядку
err-no-separator = строка { $line }: конфликт заканчивается до своего =======
err-bad-diff-line = строка { $line }: строка diff начинается не с ' ', '-' и не с '+'
err-marker-shape = строка { $line }: конфликту нужны хотя бы две стороны и на одну базу меньше; сторон: { $sides }, баз: { $bases }
err-mixed-arity = строка { $line }: конфликт с числом сторон { $found } после конфликтов с числом { $expected }
err-shape = слиянию нужны хотя бы две стороны и на одну базу меньше, чем сторон; сторон: { $sides }, баз: { $bases }
err-journal-header = в журнале нет целой строки заголовка
err-journal-line = строку { $index } журнала не удаётся разобрать: { $detail }
err-journal-version = журнал в формате { $found }, а не { $expected }
err-journal-inputs = журнал записан для других входных файлов
err-journal-alignment = журнал записан для другого выравнивания этих входных файлов, например другой версией triad
err-journal-command = команда { $index } журнала не применяется: { $detail }
err-cannot-open-window = не удаётся открыть окно: { $detail }
err-io-not-found = нет такого файла или каталога
err-io-permission = доступ запрещён
err-io-exists = файл уже существует
err-io-is-directory = это каталог
err-io-not-directory = это не каталог
err-io-full = на устройстве нет места
err-io-read-only = файловая система только для чтения

## gpui-component's text

component-replace = Заменить
component-replace-all = Заменить все
component-clear = Очистить
