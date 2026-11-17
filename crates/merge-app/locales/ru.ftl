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
menu-toggle-left = Переключить левую
menu-toggle-base = Переключить базу
menu-toggle-right = Переключить правую
menu-clear-choice = Сбросить выбор
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

## Status bar

repo-git = репозиторий git
repo-jj = репозиторий jj
repo-none = без репозитория
mode-git = слияние git
mode-jj = слияние jj
mode-git-word = слияние git по словам
mode-jj-word = слияние jj по словам
encoding-utf8-bom = UTF-8 с BOM
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
