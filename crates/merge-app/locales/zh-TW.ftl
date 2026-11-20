# The windows' text in traditional Chinese. `en.ftl` is the reference: it says how counts and
# names work. Chinese has one plural form, so counts need no selector.

## About window

about-title = 關於 triad
about-description = 三方合併與雙向比較
about-vcs-support = 為 Jujutsu 打造，完整支援 Git
about-version = 版本 { $version }
about-build-commit = 提交 { $commit }
about-build-commit-change = 提交 { $commit }，變更 { $change }

## Window parts

result-title = 結果
merge-toolbar = 合併工具
diff-toolbar = 比較工具
input-lines = 輸入行
go-to-line-field = 行號
no-matches = 無相符項目
close = 關閉
settings-errors = 設定檔有錯誤：{ $errors }

## Save, cancel and restore questions

prompt-save-unresolved = 要在衝突未解決時儲存嗎？
prompt-save-with-conflicts = 連同衝突儲存
prompt-keep-editing = 繼續編輯
prompt-discard-detail = 捨棄不會寫入任何內容，合併在 git 或 jj 中仍未解決。
prompt-discard = 捨棄
prompt-cannot-write = 無法寫入輸出
prompt-cannot-write-copy = 無法寫入副本
prompt-cannot-record = 儲存庫未記錄此次儲存
prompt-no-output = 此視窗沒有輸出檔案。
prompt-quit = 結束
prompt-try-again = 重試
prompt-restore-session = 要還原上一個工作階段嗎？
prompt-restore-detail = 這些檔案上的一個工作階段在合併解決之前結束了。可以還原其選擇與編輯。
prompt-restore = 還原
prompt-start-over = 重新開始
prompt-cannot-restore = 無法還原工作階段
prompt-save-changes = 要儲存變更嗎？
prompt-cannot-save = 無法儲存檔案
prompt-ok = 好
prompt-changed-detail = 重新載入並捨棄你的編輯，或保留你的版本稍後覆寫儲存。
prompt-reload = 重新載入
prompt-output-changed-detail = 合併開啟後，另一個程式修改了它。儲存並結束會以合併結果取代這些修改。
prompt-keep-mine = 保留我的
prompt-changed-on-disk = { $name } 在磁碟上已變更。
prompt-descendants-changed = 儲存會改變 { $count } 個後代的衝突。
prompt-unresolved-detail = { $count } 個衝突保留其標記，git 或 jj 會視合併為未解決。
prompt-unresolved-detail-git = { $count } 個衝突保留其標記，git 或 jj 會視合併為未解決。之後 git mergetool 會放回它自己的檔案副本；再次執行它即可還原此工作階段。

## Counts

count-commits = { $count } 個提交
count-files = { $count } 個檔案
count-versions = { $count } 個版本
count-hunks = { $count } 個差異區塊
count-differences = { $count } 處差異
count-conflicts = { $count } 個衝突
count-unchanged-lines = { $count } 行未變更
count-duplicated-blocks = { $count } 個重複區塊

## Conflicted commits (stack window)

no-paths = 無
stack-title = 有衝突的提交
stack-empty = 沒有有衝突的提交。
refresh = 重新整理
stack-preview = 預覽儲存
preview-running = 正在計算儲存的效果…
working-copy = 工作副本
no-description = （未設定描述）
commit-tooltip =
    變更 ID：{ $change }
    提交 ID：{ $commit }
    日期：{ $date }
inherited = （繼承）
stack-resolved = 已在 { $change } 中解決 { $path }。
stack-saved-conflicts = 已在 { $change } 中連同衝突儲存 { $path }。
stack-error = { $change } 中 { $path } 的合併以錯誤結束。
stack-inherited = { $change } 中的 { $path } 繼承了 { $origin } 的衝突。先在 { $origin } 中解決，這裡的衝突也會隨之消失；先在這裡儲存的解決很可能會再次衝突。
preview-resolves = 儲存會解決 { $change } 中的此路徑。
preview-keeps = 儲存後 { $change } 中仍有衝突。
preview-no-rebase = 沒有後代被重定基底。

## Status icons

status-added = 已新增
status-modified = 已修改
status-deleted = 已刪除
status-moved = 已移動
status-copied = 已複製
status-conflicted = 有衝突

## Changed files (directory window)

dirs-title = 已變更的檔案
dirs-empty = 沒有不同的檔案。
dirs-against = { $files }，比較 { $rev }
file-added = 已新增
file-deleted = 已刪除
file-modified = 已修改
file-renamed = 重新命名自 { $from }
file-copied = 複製自 { $from }
file-from = 來自 { $from }
file-undecodable = 無法解碼
file-binary = 二進位
file-special = 特殊檔案
file-label = { $path } { $change }
file-label-why = { $path } { $change }，{ $why }
at-operation = 操作 { $operation } 時的 @
no-file = { $name }（無檔案）

## History window

history-title = { $path } 的歷史
history-empty = 沒有版本。
version-conflict = （衝突）
version-absent = （不存在）
version-other = （其他）

## Absorb window

absorb-title = 吸收
absorb-empty = 沒有可吸收的內容。
absorb-apply = 套用
absorb-keep = 工作副本
absorb-note = 已將 { $hunks } 吸收到 { $commits } 中。
absorb-stale = 工作副本已過時：請執行 jj workspace update-stale。
absorb-skipped = 已略過 { $path }：{ $reason }

## Open dialog

open-title-merge = 開啟合併
open-title-diff = 開啟比較
mode-merge = 合併
mode-diff = 比較
start-merge = 合併
start-diff = 比較
slot-base = 基礎
slot-left = 左側
slot-right = 右側
slot-output = 輸出
choose-base = 選擇基礎檔案
choose-left = 選擇左側檔案
choose-right = 選擇右側檔案
choose-output = 選擇輸出檔案
missing-slot = 請為「{ $slot }」選擇檔案。

## Menus

menu-settings = 設定…
menu-save = 儲存
menu-save-and-exit = 儲存並結束
menu-save-copy-as = 另存副本…
menu-cancel = 取消
menu-edit = 編輯
menu-merge = 合併
menu-go = 前往
menu-view = 顯示方式
menu-undo = 復原
menu-redo = 重做
menu-find = 尋找…
menu-go-to-line = 前往行…
menu-copy-as-patch = 複製為修補檔
menu-copy-difference-as-patch = 將差異複製為修補檔
menu-toggle-left = 切換左側
menu-toggle-base = 切換基礎
menu-toggle-right = 切換右側
menu-clear-choice = 清除選擇
menu-pick-selected-lines = 選取所選行
menu-take-base = 採用基礎
menu-take-left = 採用左側
menu-take-right = 採用右側
menu-take-left-then-right = 先左後右
menu-take-right-then-left = 先右後左
menu-take-sorted-union = 採用排序聯集
menu-take-left-all-unresolved = 所有未解決處採用左側
menu-take-right-all-unresolved = 所有未解決處採用右側
menu-take-whole-left = 採用整個左側檔案
menu-take-whole-right = 採用整個右側檔案
menu-carry-moved-edit = 將編輯帶到移動的區塊
menu-drop-duplicate = 刪除重複
menu-next-difference = 下一處差異
menu-previous-difference = 上一處差異
menu-next-conflict = 下一個衝突
menu-previous-conflict = 上一個衝突
menu-go-to-move-end = 前往移動的另一端
menu-base-pane = 基礎窗格
menu-blame = 追溯
menu-syntax = 語法突顯
menu-soft-wrap = 自動換行
menu-whitespace = 顯示空白字元
menu-collapse-unchanged = 摺疊未變更
menu-unified = 統一檢視
menu-scroll-sync = 同步結果捲動

## Toolbar

tool-label-difference = 差異
tool-label-conflict = 衝突
tool-label-left = 左側
tool-label-right = 右側
more = 更多
more-menu = 更多選擇
more-base = 基礎
more-left-then-right = 先左後右
more-right-then-left = 先右後左
more-none = 無
tool-previous-difference = 上一處差異
tool-next-difference = 下一處差異
tool-previous-conflict = 上一個衝突
tool-next-conflict = 下一個衝突
tool-left = 在目前區域切換左側
tool-base = 在目前區域切換基礎
tool-right = 在目前區域切換右側
tool-left-then-right = 在目前區域採用先左後右
tool-right-then-left = 在目前區域採用先右後左
tool-clear = 清除目前區域的選擇
tool-undo = 復原
tool-redo = 重做
tool-base-pane = 顯示或隱藏基礎窗格
tool-find = 尋找
tool-go-to-line = 前往結果中的某一行
tool-syntax = 開啟或關閉語法色彩
tool-soft-wrap = 開啟或關閉自動換行
tool-whitespace = 顯示或隱藏空白字元

## Conflict badge and pick targets

pick-label = { $title }，第 { $region }/{ $regions } 個區域
pick-label-unresolved = { $title }，第 { $region }/{ $regions } 個區域，未解決
pick-label-inherited = { $title }，第 { $region }/{ $regions } 個區域，未解決，繼承
inherited-pill = 繼承
line-pick-label = { $title }，第 { $line } 行
badge-conflict = 衝突 { $number }/{ $count }
badge-sides = { $kept }/{ $sides } 方
badge-all-resolved = 全部已解決
badge-resolved-one = 衝突已解決
badge-resolved-all = { $count } 個全部已解決
badge-unresolved-of = { $unresolved }/{ $count } 未解決

## Panes

role-left = 左側
role-right = 右側
role-base = 基礎
role-side = 第 { $number } 方
role-base-of = 基礎 { $number }
role-result = 結果
cell-line = { $title }，第 { $line } 行：{ $text }
cell-no-line = { $title }，無此行
cell-no-line-deleted = { $title }，無此行：{ $deleted }
placeholder-nothing = 未選擇（捨棄 { $count } 行）
placeholder-deleted = 已刪除 { $count } 行（{ $roles }）
lines-deleted = 已刪除 { $count } 行
previous-match = { $title } 中的上一個相符項目
next-match = { $title } 中的下一個相符項目
match-count = { $number }/{ $count }
go-to-line-prompt = 前往行（1–{ $last }）：
go-to-line-current = 目前第 { $line } 行，共 { $last } 行
go-to-line-target = 前往第 { $line } 行
jj-diff-banner = 這些是來自 jj diff --tool 的暫存副本。視窗關閉後，此處的編輯將會遺失。
snapshot-failed-banner = 已儲存，但 jj 未記錄此次儲存：{ $error }
output-changed-banner = { $name } 在合併開啟後於磁碟上被修改。儲存並結束會取代它。
copy-to-right = 將此差異複製到右側
copy-to-left = 將此差異複製到左側
initial-base = 基
initial-left = 左
initial-right = 右
moved-to-base = 移動的區塊：前往其另一端，基礎第 { $line } 行
moved-to-left = 移動的區塊：前往其另一端，左側第 { $line } 行
moved-to-right = 移動的區塊：前往其另一端，右側第 { $line } 行
diff-pane-left = 左側：{ $name }
diff-pane-right = 右側：{ $name }
diff-pane-unified = 統一檢視：{ $left } 對比 { $right }

## Status bar

repo-git = git 儲存庫
repo-jj = jj 儲存庫
repo-none = 無儲存庫
mode-git = git 合併
mode-jj = jj 合併
mode-git-word = git 逐詞合併
mode-jj-word = jj 逐詞合併
encoding-utf8-bom = UTF-8 含 BOM
encoding-utf16le-bom = UTF-16 LE 含 BOM
encoding-utf16be-bom = UTF-16 BE 含 BOM
inputs-differ = { $encoding }（輸入不同）
word-merged = { $count } 處逐詞合併
unresolved = { $count } 個未解決
mixed-endings = 換行字元混用
compare-whitespace = 空白有意義
compare-line-endings = 忽略換行字元
compare-trailing = 忽略行尾空白
compare-amount = 忽略空白數量
compare-all = 忽略所有空白
case-ignored = 忽略大小寫
patterns-ignored = 忽略模式
blank-lines-ignored = 忽略空行
diverge-whitespace-jj = 空白比較方式與 jj 不同
diverge-case-git = 忽略大小寫，與 git 不同
diverge-case-jj = 忽略大小寫，與 jj 不同
diverge-patterns-git = 忽略模式，與 git 不同
diverge-patterns-jj = 忽略模式，與 jj 不同
diverge-same-change-git = 保留相同變更，與 git 不同
diverge-word-git = 逐詞合併，與 git 不同
diverge-deep-git = 衝突內部拆分，與 git 不同
diverge-deep-jj = 衝突內部拆分，與 jj 不同
diverge-syntax-git = 保留語法邊界，與 git 不同
diverge-syntax-jj = 保留語法邊界，與 jj 不同
copy-saved =
    { $count ->
        [0] 副本已儲存，已解決
       *[other] 副本已儲存，含 { $count } 個衝突
    }
saved =
    { $count ->
        [0] 已儲存，已解決
       *[other] 已儲存，含 { $count } 個衝突
    }

## Result pane's right-click menu

choice-none = 無
choice-base = 基礎
choice-left = 左側
choice-right = 右側
choice-left-then-right = 左側 + 右側
choice-right-then-left = 右側 + 左側
action-sorted-union = 排序聯集
suggested = 建議：{ $action }
suggested-carry = 建議：將{ $from }的編輯帶到第 { $line } 行
suggested-drop = 建議：刪除第 { $line } 行的重複
edit-cut = 剪下
edit-copy = 拷貝
edit-paste = 貼上
edit-select-all = 全選

## Error details

err-cannot-read = 無法讀取 { $path }：{ $detail }
err-cannot-read-plain = 無法讀取 { $path }
err-cannot-write = 無法寫入 { $path }：{ $detail }
err-cannot-save = 無法儲存 { $path }：{ $detail }
err-cannot-find = 找不到 { $path }：{ $detail }
err-in = { $path }：{ $detail }
err-at-rev = { $rev } 中的 { $path }：{ $detail }
err-not-recorded = { $path } 已寫入，但儲存庫未記錄此次儲存：{ $detail }
err-save-writing = 正在儲存合併
err-no-output = 合併沒有可寫入的輸出
err-save-running = 正在儲存
err-encode = { $encoding } 無法容納第 { $line } 行的 { $character }（U+{ $code }）
err-binary = 二進位檔案：前 { $bytes } 個位元組中有 NUL 位元組
err-invalid-utf8 = 第 { $offset } 個位元組處的 UTF-8 無效
err-invalid-utf16 = 第 { $offset } 個位元組處有未配對的 UTF-16 代理字元
err-odd-utf16 = 位元組數為奇數的 UTF-16 檔案
err-not-a-repository = { $path } 不在 git 或 jj 儲存庫中
err-not-utf8-path = { $path } 不是 UTF-8 路徑
err-no-working-copy = { $path } 沒有工作副本
err-outside-repository = { $path } 在儲存庫之外
err-stale-file = 磁碟上的 { $path } 不是工作副本提交中的版本；請執行 jj workspace update-stale 後重試
err-stale-workspace = { $path } 中的工作副本已過時；請執行 jj workspace update-stale 後重試
err-unknown-revision = 未知的修訂 { $rev }
err-ambiguous-revision = 修訂 { $rev } 有歧義
err-conflicted-at = { $path } 在 { $rev } 中有衝突
err-no-index = { $name } 沒有索引
err-immutable = 提交 { $rev } 不可變更
err-not-conflicted-at = { $path } 在 { $rev } 中沒有衝突
err-bad-config = { $key } 的值 { $value } 未知
err-backend = { $context }：{ $detail }
err-concurrent-operations = 儲存庫有 { $count } 個並行操作；任何 jj 命令都會協調它們
err-not-a-file-conflict = { $path } 不是檔案之間的衝突
err-unknown-operation = 沒有操作 { $op }：{ $reason }
err-no-working-copy-at = 工作區在操作 { $op } 時沒有工作副本
err-no-hunk = 沒有差異區塊 { $hunk }；共有 { $count } 個
err-not-a-destination = --into 中的 { $rev } 不是工作副本的可變祖先
err-repository-moved = 自操作 { $op } 以來儲存庫已變更；請重新列出差異區塊
err-conflict-changed = { $change } 中的 { $path } 在 triad 讀取後已變更；未寫入任何內容
err-sides-changed = { $path }：jj 把一個 { $before } 方的衝突讀回為 { $after } 方
err-jj-config-load = 無法讀取 jj 設定 { $path }：{ $detail }
err-jj-config-id = { $path }：不是 jj 設定 ID
err-jj-config-value = jj 設定中的 { $key }：{ $detail }
err-jj-config-value-in = { $path }：jj 設定中的 { $key }：{ $detail }
err-jj-config-get = 讀取 jj 設定值
err-jj-config-set = 設定 jj 設定值
err-jj-config-find = 尋找儲存庫或工作區的 jj 設定
err-auto-track = `snapshot.auto-track = { $text }` 不是 fileset
err-git-ignores = 讀取 git 的忽略檔案
err-jj-workspace = 載入 jj 工作區
err-needs-jj = { $command } 需要 jj 儲存庫
err-not-conflicted = { $path } 沒有衝突
err-term-not-text = { $path }：{ $term } 不是文字：{ $detail }
err-term-not-a-file = { $path }：{ $term } 是{ $kind }；只能解決文字衝突
err-kind-symlink = 符號連結
err-kind-submodule = 子模組
err-kind-directory = 目錄
err-deleted-everywhere = { $path } 在每一方都已刪除；沒有可合併的內容
err-resolved-in-tree = { $path } 在工作樹中已沒有衝突標記，看起來已解決；請將其標記為已解決，或放回標記以重新合併
err-tree-not-text = { $path }：工作樹中的檔案不是文字（{ $detail }），可能含有儲存會覆寫的編輯；請放回帶衝突標記的文字，或手動解決
err-temporary = 無法為結果建立暫存檔：{ $detail }
err-not-a-directory = { $path } 不是目錄
err-mixed = diff 需要兩個檔案或兩個目錄：{ $dir } 是目錄，{ $other } 不是
err-git-in-jj = { $path } 是 jj 儲存庫，triad 在此只按 jj 的語意合併；請用 `jj resolve` 解決
err-git-in-jj-resolve = { $path } 是 jj 的解決目錄，triad 在此只按 jj 的語意合併；請用 `jj resolve` 解決
err-no-markers = 沒有衝突標記
err-unclosed = 第 { $line } 行：衝突未關閉
err-nested = 第 { $line } 行：衝突在另一個衝突內部開始
err-out-of-order = 第 { $line } 行：標記順序錯誤
err-no-separator = 第 { $line } 行：衝突在其 ======= 之前結束
err-bad-diff-line = 第 { $line } 行：diff 行不以 ' '、'-' 或 '+' 開頭
err-marker-shape = 第 { $line } 行：衝突至少需要兩方，且基礎比方數少一個，實際為 { $sides } 方、{ $bases } 個基礎
err-mixed-arity = 第 { $line } 行：在 { $expected } 方的衝突之後出現 { $found } 方的衝突
err-shape = 合併至少需要兩方，且基礎比方數少一個，實際為 { $sides } 方、{ $bases } 個基礎
err-journal-header = 日誌沒有完整的標頭行
err-journal-line = 無法解析日誌第 { $index } 行：{ $detail }
err-journal-version = 日誌格式為 { $found }，而不是 { $expected }
err-journal-inputs = 日誌是為其他輸入寫的
err-journal-alignment = 日誌是為這些輸入的另一種對齊寫的，例如來自另一個版本的 triad
err-journal-command = 無法套用日誌中的命令 { $index }：{ $detail }
err-cannot-open-window = 無法開啟視窗：{ $detail }
err-io-not-found = 沒有該檔案或目錄
err-io-permission = 權限被拒
err-io-exists = 檔案已存在
err-io-is-directory = 這是一個目錄
err-io-not-directory = 不是目錄
err-io-full = 裝置上沒有剩餘空間
err-io-read-only = 檔案系統為唯讀

## gpui-component's text

component-replace = 取代
component-replace-all = 全部取代
component-clear = 清除
