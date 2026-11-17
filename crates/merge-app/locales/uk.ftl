# The windows' text in Ukrainian. `en.ftl` is the reference: it says how counts and names
# work. Ukrainian counts take three forms: `[one]` (1, 21, 31…), `[few]` (2–4, 22–24…) and
# `[many]` (0, 5–20, 25–30…). `[many]` is the default.

## About window

about-title = Про програму triad
about-description = Тристороннє злиття і двостороннє порівняння
about-vcs-support = Створено для Jujutsu, з повною підтримкою Git
about-version = Версія { $version }
about-build-commit = Коміт { $commit }
about-build-commit-change = Коміт { $commit }, зміна { $change }

## Window parts

result-title = Результат
merge-toolbar = Інструменти злиття
diff-toolbar = Інструменти порівняння
input-lines = Вхідні рядки
go-to-line-field = Номер рядка
no-matches = Немає збігів
close = Закрити
settings-errors = У файлі налаштувань є помилки: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Зберегти з нерозв’язаними конфліктами?
prompt-save-with-conflicts = Зберегти з конфліктами
prompt-keep-editing = Продовжити редагування
prompt-discard-detail = «Відкинути» нічого не записує, і злиття лишається нерозв’язаним у git або jj.
prompt-discard = Відкинути
prompt-cannot-write = Не вдається записати результат
prompt-cannot-write-copy = Не вдається записати копію
prompt-cannot-record = Репозиторій не записав збереження
prompt-no-output = У цього вікна немає вихідного файлу.
prompt-quit = Вийти
prompt-try-again = Повторити
prompt-restore-session = Відновити попередній сеанс?
prompt-restore-detail = Сеанс роботи з цими файлами завершився до розв’язання злиття. Його вибір і правки можна повернути.
prompt-restore = Відновити
prompt-start-over = Почати заново
prompt-cannot-restore = Не вдається відновити сеанс
prompt-save-changes = Зберегти зміни?
prompt-cannot-save = Не вдається зберегти файл
prompt-ok = Гаразд
prompt-changed-detail = Перезавантажте файл і втратьте правки або залиште свою версію і збережіть її поверх пізніше.
prompt-reload = Перезавантажити
prompt-keep-mine = Залишити мою
prompt-changed-on-disk = Файл { $name } змінився на диску.
prompt-descendants-changed =
    { $count ->
        [one] Збереження змінює конфлікти { $count } нащадка.
       *[many] Збереження змінює конфлікти { $count } нащадків.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } конфлікт зберігає свої маркери, і git або jj вважають злиття нерозв’язаним.
        [few] { $count } конфлікти зберігають свої маркери, і git або jj вважають злиття нерозв’язаним.
       *[many] { $count } конфліктів зберігають свої маркери, і git або jj вважають злиття нерозв’язаним.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } конфлікт зберігає свої маркери, і git або jj вважають злиття нерозв’язаним. Потім git mergetool поверне свою копію файлу; запустіть його знову, щоб відновити цей сеанс.
        [few] { $count } конфлікти зберігають свої маркери, і git або jj вважають злиття нерозв’язаним. Потім git mergetool поверне свою копію файлу; запустіть його знову, щоб відновити цей сеанс.
       *[many] { $count } конфліктів зберігають свої маркери, і git або jj вважають злиття нерозв’язаним. Потім git mergetool поверне свою копію файлу; запустіть його знову, щоб відновити цей сеанс.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } коміт
        [few] { $count } коміти
       *[many] { $count } комітів
    }
count-files =
    { $count ->
        [one] { $count } файл
        [few] { $count } файли
       *[many] { $count } файлів
    }
count-versions =
    { $count ->
        [one] { $count } версія
        [few] { $count } версії
       *[many] { $count } версій
    }
count-hunks =
    { $count ->
        [one] { $count } фрагмент
        [few] { $count } фрагменти
       *[many] { $count } фрагментів
    }
count-differences =
    { $count ->
        [one] { $count } відмінність
        [few] { $count } відмінності
       *[many] { $count } відмінностей
    }
count-conflicts =
    { $count ->
        [one] { $count } конфлікт
        [few] { $count } конфлікти
       *[many] { $count } конфліктів
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } незмінений рядок
        [few] { $count } незмінені рядки
       *[many] { $count } незмінених рядків
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } повторений блок
        [few] { $count } повторені блоки
       *[many] { $count } повторених блоків
    }

## Conflicted commits (stack window)

no-paths = немає
stack-title = Коміти з конфліктами
stack-empty = Немає комітів з конфліктами.
refresh = Оновити
stack-preview = Попередній перегляд збереження
preview-running = Обчислюється результат збереження…
working-copy = робоча копія
no-description = (опис не задано)
commit-tooltip =
    ID зміни: { $change }
    ID коміту: { $commit }
    Дата: { $date }
inherited = (успадкований)
stack-resolved = { $path } розв’язано в { $change }.
stack-saved-conflicts = { $path } збережено в { $change } з конфліктами.
stack-error = Злиття { $path } у { $change } завершилося помилкою.
preview-resolves = Збереження розв’яже шлях у { $change }.
preview-keeps = Після збереження в { $change } лишаться конфлікти.
preview-no-rebase = Жоден нащадок не перебазовується.

## Status icons

status-added = Додано
status-modified = Змінено
status-deleted = Видалено
status-moved = Переміщено
status-copied = Скопійовано
status-conflicted = Конфлікт

## Changed files (directory window)

dirs-title = Змінені файли
dirs-empty = Файлів, що відрізняються, немає.
dirs-against = { $files } відносно { $rev }
file-added = додано
file-deleted = видалено
file-modified = змінено
file-renamed = перейменовано з { $from }
file-copied = скопійовано з { $from }
file-from = з { $from }
file-undecodable = не декодується
file-binary = двійковий
file-special = особливий
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ на операції { $operation }
no-file = { $name } (немає файлу)

## History window

history-title = Історія { $path }
history-empty = Немає версій.
version-conflict = (конфлікт)
version-absent = (відсутній)
version-other = (інше)

## Absorb window

absorb-title = Поглинання
absorb-empty = Нічого поглинати.
absorb-apply = Застосувати
absorb-keep = Робоча копія
absorb-note = Поглинуто: { $hunks } у { $commits }.
absorb-stale = Робоча копія застаріла: виконайте jj workspace update-stale.
absorb-skipped = Пропущено { $path }: { $reason }

## Open dialog

open-title-merge = Відкрити злиття
open-title-diff = Відкрити порівняння
mode-merge = Злиття
mode-diff = Порівняння
start-merge = Злити
start-diff = Порівняти
slot-base = База
slot-left = Ліворуч
slot-right = Праворуч
slot-output = Результат
choose-base = Вибрати базовий файл
choose-left = Вибрати лівий файл
choose-right = Вибрати правий файл
choose-output = Вибрати вихідний файл
missing-slot = Виберіть файл для «{ $slot }».

## Menus

menu-settings = Налаштування…
menu-save = Зберегти
menu-save-and-exit = Зберегти і вийти
menu-save-copy-as = Зберегти копію як…
menu-cancel = Скасувати
menu-edit = Редагування
menu-merge = Злиття
menu-go = Перехід
menu-view = Вигляд
menu-undo = Скасувати дію
menu-redo = Повторити
menu-find = Знайти…
menu-go-to-line = Перейти до рядка…
menu-toggle-left = Перемкнути ліву
menu-toggle-base = Перемкнути базу
menu-toggle-right = Перемкнути праву
menu-clear-choice = Скинути вибір
menu-take-base = Взяти базу
menu-take-left = Взяти ліву
menu-take-right = Взяти праву
menu-take-left-then-right = Взяти ліву, потім праву
menu-take-right-then-left = Взяти праву, потім ліву
menu-take-sorted-union = Взяти відсортоване об’єднання
menu-take-left-all-unresolved = Взяти ліву в усіх нерозв’язаних
menu-take-right-all-unresolved = Взяти праву в усіх нерозв’язаних
menu-take-whole-left = Взяти весь лівий файл
menu-take-whole-right = Взяти весь правий файл
menu-carry-moved-edit = Перенести правку в переміщений блок
menu-drop-duplicate = Видалити повтор
menu-next-difference = Наступна відмінність
menu-previous-difference = Попередня відмінність
menu-next-conflict = Наступний конфлікт
menu-previous-conflict = Попередній конфлікт
menu-go-to-move-end = До іншого кінця переміщення
menu-base-pane = Панель бази
menu-blame = Походження
menu-syntax = Підсвічування синтаксису
menu-soft-wrap = Перенесення рядків
menu-whitespace = Показувати пробіли
menu-collapse-unchanged = Згорнути незмінене
menu-scroll-sync = Синхронне прокручування результату

## Toolbar

tool-label-difference = Відмінність
tool-label-conflict = Конфлікт
tool-label-left = Ліворуч
tool-label-right = Праворуч
more = Ще
more-menu = Інші варіанти
more-base = База
more-left-then-right = Ліворуч, потім праворуч
more-right-then-left = Праворуч, потім ліворуч
more-none = Нічого
tool-previous-difference = Попередня відмінність
tool-next-difference = Наступна відмінність
tool-previous-conflict = Попередній конфлікт
tool-next-conflict = Наступний конфлікт
tool-left = Перемкнути ліву в поточній області
tool-base = Перемкнути базу в поточній області
tool-right = Перемкнути праву в поточній області
tool-left-then-right = Взяти ліву, потім праву в поточній області
tool-right-then-left = Взяти праву, потім ліву в поточній області
tool-clear = Скинути вибір поточної області
tool-undo = Скасувати дію
tool-redo = Повторити
tool-base-pane = Показати або сховати панель бази
tool-find = Знайти
tool-go-to-line = Перейти до рядка результату
tool-syntax = Увімкнути або вимкнути кольори синтаксису
tool-soft-wrap = Увімкнути або вимкнути перенесення рядків
tool-whitespace = Показати або сховати пробіли

## Conflict badge and pick targets

pick-label = { $title } в області { $region } з { $regions }
pick-label-unresolved = { $title } в області { $region } з { $regions }, не розв’язано
badge-conflict = Конфлікт { $number } з { $count }
badge-sides = { $kept } з { $sides } сторін
badge-all-resolved = усі розв’язано
badge-resolved-one = Конфлікт розв’язано
badge-resolved-all = Усі { $count } розв’язано
badge-unresolved-of = Не розв’язано: { $unresolved } з { $count }

## Panes

role-left = Ліворуч
role-right = Праворуч
role-base = База
role-side = Сторона { $number }
role-base-of = База { $number }
role-result = Результат
cell-line = { $title }, рядок { $line }: { $text }
cell-no-line = { $title }, немає рядка
cell-no-line-deleted = { $title }, немає рядка: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Нічого не вибрано (відкинуто { $count } рядок)
        [few] Нічого не вибрано (відкинуто { $count } рядки)
       *[many] Нічого не вибрано (відкинуто { $count } рядків)
    }
placeholder-deleted =
    { $count ->
        [one] Видалено { $count } рядок ({ $roles })
        [few] Видалено { $count } рядки ({ $roles })
       *[many] Видалено { $count } рядків ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] Видалено { $count } рядок
        [few] Видалено { $count } рядки
       *[many] Видалено { $count } рядків
    }
previous-match = Попередній збіг у «{ $title }»
next-match = Наступний збіг у «{ $title }»
match-count = { $number } з { $count }
go-to-line-prompt = Перейти до рядка (1–{ $last }):
go-to-line-current = Поточний рядок { $line } з { $last }
go-to-line-target = Перейти до рядка { $line }
jj-diff-banner = Це тимчасові копії від jj diff --tool. Правки тут зникнуть, коли вікно закриється.
copy-to-right = Скопіювати цю відмінність праворуч
copy-to-left = Скопіювати цю відмінність ліворуч
initial-base = Б
initial-left = Л
initial-right = П
moved-to-base = Переміщений блок: до іншого кінця, рядок { $line } бази
moved-to-left = Переміщений блок: до іншого кінця, рядок { $line } ліворуч
moved-to-right = Переміщений блок: до іншого кінця, рядок { $line } праворуч
diff-pane-left = Ліворуч: { $name }
diff-pane-right = Праворуч: { $name }

## Status bar

repo-git = репозиторій git
repo-jj = репозиторій jj
repo-none = без репозиторію
mode-git = злиття git
mode-jj = злиття jj
mode-git-word = злиття git за словами
mode-jj-word = злиття jj за словами
encoding-utf8-bom = UTF-8 з BOM
inputs-differ = { $encoding } (входи відрізняються)
word-merged = Злито за словами: { $count }
unresolved = Не розв’язано: { $count }
mixed-endings = змішані кінці рядків
compare-whitespace = пробіли значущі
compare-line-endings = кінці рядків ігноруються
compare-trailing = пробіли в кінці рядків ігноруються
compare-amount = кількість пробілів ігнорується
compare-all = усі пробіли ігноруються
case-ignored = регістр ігнорується
patterns-ignored = шаблони ігноруються
blank-lines-ignored = порожні рядки ігноруються
diverge-whitespace-jj = пробіли порівнюються не як у jj
diverge-case-git = регістр ігнорується, на відміну від git
diverge-case-jj = регістр ігнорується, на відміну від jj
diverge-patterns-git = шаблони ігноруються, на відміну від git
diverge-patterns-jj = шаблони ігноруються, на відміну від jj
diverge-same-change-git = однакові зміни збережено, на відміну від git
diverge-word-git = слова злито, на відміну від git
diverge-deep-git = конфлікти розділено всередині, на відміну від git
diverge-deep-jj = конфлікти розділено всередині, на відміну від jj
diverge-syntax-git = межі синтаксису дотримано, на відміну від git
diverge-syntax-jj = межі синтаксису дотримано, на відміну від jj
copy-saved =
    { $count ->
        [0] Копію збережено, усе розв’язано
        [one] Копію збережено з { $count } конфліктом
       *[many] Копію збережено з { $count } конфліктами
    }

## Result pane's right-click menu

choice-none = Нічого
choice-base = База
choice-left = Ліворуч
choice-right = Праворуч
choice-left-then-right = Ліворуч + праворуч
choice-right-then-left = Праворуч + ліворуч
action-sorted-union = Відсортоване об’єднання
suggested = Пропозиція: { $action }
suggested-carry = Пропозиція: перенести правку «{ $from }» у рядок { $line }
suggested-drop = Пропозиція: видалити повтор рядка { $line }
edit-cut = Вирізати
edit-copy = Копіювати
edit-paste = Вставити
edit-select-all = Виділити все
