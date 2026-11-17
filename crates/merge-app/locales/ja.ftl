# The windows' text in Japanese. `en.ftl` is the reference: it says how counts and names
# work. Japanese has one plural form, so counts need no selector.

## About window

about-title = triad について
about-description = 3 方向マージと 2 方向比較
about-vcs-support = Jujutsu 向けに設計、Git も完全サポート
about-version = バージョン { $version }
about-build-commit = コミット { $commit }
about-build-commit-change = コミット { $commit }、変更 { $change }

## Window parts

result-title = 結果
merge-toolbar = マージツール
diff-toolbar = 比較ツール
input-lines = 入力行
go-to-line-field = 行番号
no-matches = 一致なし
close = 閉じる
settings-errors = 設定ファイルにエラーがあります: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = 未解決の競合を残したまま保存しますか？
prompt-save-with-conflicts = 競合を残して保存
prompt-keep-editing = 編集を続ける
prompt-discard-detail = 破棄すると何も書き込まれず、マージは git または jj で未解決のままです。
prompt-discard = 破棄
prompt-cannot-write = 出力を書き込めません
prompt-cannot-write-copy = コピーを書き込めません
prompt-cannot-record = リポジトリが保存を記録しませんでした
prompt-no-output = このウィンドウには出力ファイルがありません。
prompt-quit = 終了
prompt-try-again = 再試行
prompt-restore-session = 前回のセッションを復元しますか？
prompt-restore-detail = これらのファイルのセッションが、マージの解決前に終了しました。選択と編集を復元できます。
prompt-restore = 復元
prompt-start-over = 最初からやり直す
prompt-cannot-restore = セッションを復元できません
prompt-save-changes = 変更を保存しますか？
prompt-cannot-save = ファイルを保存できません
prompt-ok = OK
prompt-changed-detail = 再読み込みして編集を破棄するか、自分の版を保持して後で上書き保存します。
prompt-reload = 再読み込み
prompt-keep-mine = 自分の版を保持
prompt-changed-on-disk = { $name } がディスク上で変更されました。
prompt-descendants-changed = 保存すると { $count } 個の子孫の競合が変わります。
prompt-unresolved-detail = { $count } 件の競合がマーカーを残し、git や jj はマージを未解決と見なします。
prompt-unresolved-detail-git = { $count } 件の競合がマーカーを残し、git や jj はマージを未解決と見なします。その後 git mergetool が自身のファイルのコピーを戻すので、このセッションを復元するにはもう一度実行してください。

## Counts

count-commits = { $count } 件のコミット
count-files = { $count } 個のファイル
count-versions = { $count } 個のバージョン
count-hunks = { $count } 個のハンク
count-differences = { $count } 件の差分
count-conflicts = { $count } 件の競合
count-unchanged-lines = 変更のない { $count } 行
count-duplicated-blocks = { $count } 個の重複ブロック

## Conflicted commits (stack window)

no-paths = なし
stack-title = 競合のあるコミット
stack-empty = 競合のあるコミットはありません。
refresh = 更新
stack-preview = 保存をプレビュー
preview-running = 保存の結果を計算中…
working-copy = 作業コピー
no-description = （説明なし）
commit-tooltip =
    変更 ID: { $change }
    コミット ID: { $commit }
    日付: { $date }
inherited = （継承）
stack-resolved = { $change } の { $path } を解決しました。
stack-saved-conflicts = { $change } の { $path } を競合を残して保存しました。
stack-error = { $change } の { $path } のマージはエラーで終了しました。
preview-resolves = 保存すると { $change } のこのパスは解決します。
preview-keeps = 保存しても { $change } に競合が残ります。
preview-no-rebase = リベースされる子孫はありません。

## Status icons

status-added = 追加
status-modified = 変更
status-deleted = 削除
status-moved = 移動
status-copied = コピー
status-conflicted = 競合

## Changed files (directory window)

dirs-title = 変更されたファイル
dirs-empty = 異なるファイルはありません。
dirs-against = { $files }（{ $rev } と比較）
file-added = 追加
file-deleted = 削除
file-modified = 変更
file-renamed = { $from } から名前変更
file-copied = { $from } からコピー
file-from = { $from } から
file-undecodable = デコード不可
file-binary = バイナリ
file-special = 特殊ファイル
file-label = { $path } { $change }
file-label-why = { $path } { $change }、{ $why }
at-operation = 操作 { $operation } 時点の @
no-file = { $name }（ファイルなし）

## History window

history-title = { $path } の履歴
history-empty = バージョンはありません。
version-conflict = （競合）
version-absent = （なし）
version-other = （その他）

## Absorb window

absorb-title = 吸収
absorb-empty = 吸収するものはありません。
absorb-apply = 適用
absorb-keep = 作業コピー
absorb-note = { $hunks } を { $commits } に吸収しました。
absorb-stale = 作業コピーが古くなっています: jj workspace update-stale を実行してください。
absorb-skipped = { $path } をスキップ: { $reason }

## Open dialog

open-title-merge = マージを開く
open-title-diff = 比較を開く
mode-merge = マージ
mode-diff = 比較
start-merge = マージ
start-diff = 比較
slot-base = ベース
slot-left = 左
slot-right = 右
slot-output = 出力
choose-base = ベースファイルを選択
choose-left = 左のファイルを選択
choose-right = 右のファイルを選択
choose-output = 出力ファイルを選択
missing-slot = 「{ $slot }」のファイルを選択してください。

## Menus

menu-settings = 設定…
menu-save = 保存
menu-save-and-exit = 保存して終了
menu-save-copy-as = コピーを別名で保存…
menu-cancel = キャンセル
menu-edit = 編集
menu-merge = マージ
menu-go = 移動
menu-view = 表示
menu-undo = 元に戻す
menu-redo = やり直す
menu-find = 検索…
menu-go-to-line = 行へ移動…
menu-toggle-left = 左を切り替え
menu-toggle-base = ベースを切り替え
menu-toggle-right = 右を切り替え
menu-clear-choice = 選択を解除
menu-take-base = ベースを採用
menu-take-left = 左を採用
menu-take-right = 右を採用
menu-take-left-then-right = 左、次に右を採用
menu-take-right-then-left = 右、次に左を採用
menu-take-sorted-union = ソートした和集合を採用
menu-take-left-all-unresolved = 未解決のすべてで左を採用
menu-take-right-all-unresolved = 未解決のすべてで右を採用
menu-take-whole-left = 左のファイル全体を採用
menu-take-whole-right = 右のファイル全体を採用
menu-carry-moved-edit = 編集を移動先のブロックへ反映
menu-drop-duplicate = 重複を削除
menu-next-difference = 次の差分
menu-previous-difference = 前の差分
menu-next-conflict = 次の競合
menu-previous-conflict = 前の競合
menu-go-to-move-end = 移動のもう一方の端へ
menu-base-pane = ベースペイン
menu-blame = 行の由来
menu-syntax = シンタックスハイライト
menu-soft-wrap = ソフトラップ
menu-whitespace = 空白を表示
menu-collapse-unchanged = 変更のない行を折りたたむ
menu-scroll-sync = 結果のスクロールを同期

## Toolbar

tool-label-difference = 差分
tool-label-conflict = 競合
tool-label-left = 左
tool-label-right = 右
more = その他
more-menu = その他の選択
more-base = ベース
more-left-then-right = 左、次に右
more-right-then-left = 右、次に左
more-none = なし
tool-previous-difference = 前の差分
tool-next-difference = 次の差分
tool-previous-conflict = 前の競合
tool-next-conflict = 次の競合
tool-left = 現在の領域で左を切り替え
tool-base = 現在の領域でベースを切り替え
tool-right = 現在の領域で右を切り替え
tool-left-then-right = 現在の領域で左、次に右を採用
tool-right-then-left = 現在の領域で右、次に左を採用
tool-clear = 現在の領域の選択を解除
tool-undo = 元に戻す
tool-redo = やり直す
tool-base-pane = ベースペインの表示を切り替え
tool-find = 検索
tool-go-to-line = 結果の行へ移動
tool-syntax = 構文の色分けを切り替え
tool-soft-wrap = ソフトラップを切り替え
tool-whitespace = 空白の表示を切り替え

## Conflict badge and pick targets

pick-label = { $title }、領域 { $region }/{ $regions }
pick-label-unresolved = { $title }、領域 { $region }/{ $regions }、未解決
badge-conflict = 競合 { $number }/{ $count }
badge-sides = { $kept }/{ $sides } 側
badge-all-resolved = すべて解決済み
badge-resolved-one = 競合を解決済み
badge-resolved-all = { $count } 件すべて解決済み
badge-unresolved-of = { $unresolved }/{ $count } 件未解決

## Panes

role-left = 左
role-right = 右
role-base = ベース
role-side = サイド { $number }
role-base-of = ベース { $number }
role-result = 結果
cell-line = { $title }、{ $line } 行目: { $text }
cell-no-line = { $title }、行なし
cell-no-line-deleted = { $title }、行なし: { $deleted }
placeholder-nothing = 選択なし（{ $count } 行を破棄）
placeholder-deleted = { $count } 行削除（{ $roles }）
lines-deleted = { $count } 行削除
previous-match = { $title } の前の一致
next-match = { $title } の次の一致
match-count = { $number }/{ $count }
go-to-line-prompt = 行へ移動（1–{ $last }）:
go-to-line-current = 現在の行 { $line }/{ $last }
go-to-line-target = { $line } 行目へ移動
jj-diff-banner = これらは jj diff --tool による一時コピーです。ウィンドウを閉じるとここでの編集は失われます。
copy-to-right = この差分を右へコピー
copy-to-left = この差分を左へコピー
initial-base = B
initial-left = L
initial-right = R
moved-to-base = 移動したブロック: もう一方の端、ベースの { $line } 行目へ
moved-to-left = 移動したブロック: もう一方の端、左の { $line } 行目へ
moved-to-right = 移動したブロック: もう一方の端、右の { $line } 行目へ
diff-pane-left = 左: { $name }
diff-pane-right = 右: { $name }

## Status bar

repo-git = git リポジトリ
repo-jj = jj リポジトリ
repo-none = リポジトリなし
mode-git = git マージ
mode-jj = jj マージ
mode-git-word = git 単語単位マージ
mode-jj-word = jj 単語単位マージ
encoding-utf8-bom = UTF-8 BOM 付き
inputs-differ = { $encoding }（入力が異なる）
word-merged = { $count } 件を単語単位でマージ
unresolved = { $count } 件未解決
mixed-endings = 改行コード混在
compare-whitespace = 空白を区別
compare-line-endings = 改行コードを無視
compare-trailing = 行末の空白を無視
compare-amount = 空白の量を無視
compare-all = すべての空白を無視
case-ignored = 大文字小文字を無視
patterns-ignored = パターンを無視
blank-lines-ignored = 空行を無視
diverge-whitespace-jj = 空白の比較が jj と異なる
diverge-case-git = 大文字小文字を無視（git と異なる）
diverge-case-jj = 大文字小文字を無視（jj と異なる）
diverge-patterns-git = パターンを無視（git と異なる）
diverge-patterns-jj = パターンを無視（jj と異なる）
diverge-same-change-git = 同じ変更を保持（git と異なる）
diverge-word-git = 単語単位でマージ（git と異なる）
diverge-deep-git = 競合を内部で分割（git と異なる）
diverge-deep-jj = 競合を内部で分割（jj と異なる）
diverge-syntax-git = 構文の境界を維持（git と異なる）
diverge-syntax-jj = 構文の境界を維持（jj と異なる）
copy-saved =
    { $count ->
        [0] コピーを保存しました（解決済み）
       *[other] コピーを保存しました（競合 { $count } 件）
    }

## Result pane's right-click menu

choice-none = なし
choice-base = ベース
choice-left = 左
choice-right = 右
choice-left-then-right = 左 + 右
choice-right-then-left = 右 + 左
action-sorted-union = ソートした和集合
suggested = 提案: { $action }
suggested-carry = 提案: { $from } の編集を { $line } 行目へ反映
suggested-drop = 提案: { $line } 行目の重複を削除
edit-cut = カット
edit-copy = コピー
edit-paste = ペースト
edit-select-all = すべてを選択
