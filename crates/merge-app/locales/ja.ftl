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
prompt-output-changed-detail = マージを開いた後に別のプログラムが変更しました。保存して終了すると、その変更はマージで置き換えられます。
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
stack-inherited = { $change } の { $path } は { $origin } からコンフリクトを引き継いでいます。先に { $origin } で解決すると、ここでも解消されます。先にここで保存した解決は、再びコンフリクトする可能性が高いです。
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
evolog-title = { $change } のバージョン
interdiff-title = { $short } 以降の { $change }
dirs-since = { $files }（{ $short } 以降）
interdiff-rebased = { $short } 時点の { $change }（リベース済み）
interdiff-version = { $short } 時点の { $change }
evolog-no-difference = 現在のバージョンとの差分はありません。
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
menu-copy-as-patch = パッチとしてコピー
menu-copy-difference-as-patch = 差分をパッチとしてコピー
menu-toggle-left = 左を切り替え
menu-toggle-base = ベースを切り替え
menu-toggle-right = 右を切り替え
menu-clear-choice = 選択を解除
menu-pick-selected-lines = 選択した行を採用
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
menu-unified = 統合表示
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
pick-label-inherited = { $title }、領域 { $region }/{ $regions }、未解決、継承
inherited-pill = 継承
line-pick-label = { $title }、{ $line } 行目
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
snapshot-failed-banner = 保存しましたが、jj は保存を記録しませんでした: { $error }
output-changed-banner = { $name } はマージを開いた後にディスク上で変更されました。保存して終了すると置き換えられます。
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
diff-pane-unified = 統合表示: { $left } と { $right }

## Status bar

repo-git = git リポジトリ
repo-jj = jj リポジトリ
repo-none = リポジトリなし
mode-git = git マージ
mode-jj = jj マージ
mode-git-word = git 単語単位マージ
mode-jj-word = jj 単語単位マージ
encoding-utf8-bom = UTF-8 BOM 付き
encoding-utf16le-bom = UTF-16 LE BOM 付き
encoding-utf16be-bom = UTF-16 BE BOM 付き
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
saved =
    { $count ->
        [0] 保存しました（解決済み）
       *[other] 保存しました（競合 { $count } 件）
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

## Error details

err-cannot-read = { $path } を読み込めません: { $detail }
err-cannot-read-plain = { $path } を読み込めません
err-cannot-write = { $path } に書き込めません: { $detail }
err-cannot-save = { $path } を保存できません: { $detail }
err-cannot-find = { $path } が見つかりません: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $rev } の { $path }: { $detail }
err-not-recorded = { $path } は書き込まれましたが、リポジトリは保存を記録しませんでした: { $detail }
err-save-writing = 保存がマージを書き込み中です
err-no-output = マージに書き込む出力がありません
err-save-running = 保存を実行中です
err-encode = { $encoding } では { $line } 行目の { $character } (U+{ $code }) を表せません
err-binary = バイナリファイル: 先頭 { $bytes } バイト以内に NUL バイトがあります
err-invalid-utf8 = バイト { $offset } に不正な UTF-8 があります
err-invalid-utf16 = バイト { $offset } に対のない UTF-16 サロゲートがあります
err-odd-utf16 = バイト数が奇数の UTF-16 ファイルです
err-not-a-repository = { $path } は git または jj のリポジトリ内にありません
err-not-utf8-path = { $path } は UTF-8 のパスではありません
err-no-working-copy = { $path } には作業コピーがありません
err-outside-repository = { $path } はリポジトリの外にあります
err-stale-file = ディスク上の { $path } は作業コピーのコミットのものではありません。jj workspace update-stale を実行してからやり直してください
err-stale-workspace = { $path } の作業コピーは古くなっています。jj workspace update-stale を実行してからやり直してください
err-unknown-revision = 不明なリビジョン { $rev }
err-ambiguous-revision = リビジョン { $rev } はあいまいです
err-conflicted-at = { $path } は { $rev } で競合しています
err-no-index = { $name } にはインデックスがありません
err-immutable = コミット { $rev } は変更できません
err-not-conflicted-at = { $path } は { $rev } で競合していません
err-bad-config = { $key } の値 { $value } は不明です
err-backend = { $context }: { $detail }
err-concurrent-operations = リポジトリに並行する操作が { $count } 件あります。jj のどのコマンドでも統合されます
err-not-a-file-conflict = { $path } はファイルの競合ではありません
err-unknown-operation = 操作 { $op } はありません: { $reason }
err-no-working-copy-at = 操作 { $op } の時点でワークスペースに作業コピーがありません
err-no-hunk = ハンク { $hunk } はありません。ハンクは { $count } 件です
err-not-a-destination = --into の { $rev } は作業コピーの変更可能な祖先ではありません
err-repository-moved = 操作 { $op } 以降にリポジトリが変更されました。ハンクを一覧し直してください
err-conflict-changed = { $change } の { $path } は triad が読み込んだ後に変更されました。何も書き込みません
err-sides-changed = { $path }: jj は { $before } 面の競合を { $after } 面の競合として読み戻しました
err-jj-config-load = jj の設定 { $path } を読み込めません: { $detail }
err-jj-config-id = { $path }: jj の設定 ID ではありません
err-jj-config-value = jj の設定の { $key }: { $detail }
err-jj-config-value-in = { $path }: jj の設定の { $key }: { $detail }
err-jj-config-get = jj の設定値の読み込み
err-jj-config-set = jj の設定値の書き込み
err-jj-config-find = リポジトリまたはワークスペースの jj 設定の検索
err-auto-track = `snapshot.auto-track = { $text }` は fileset ではありません
err-git-ignores = git の除外ファイルの読み込み
err-jj-workspace = jj ワークスペースの読み込み
err-needs-jj = { $command } には jj リポジトリが必要です
err-not-conflicted = { $path } は競合していません
err-term-not-text = { $path }: { $term } はテキストではありません: { $detail }
err-term-not-a-file = { $path }: { $term } は{ $kind }です。解決できるのはテキストの競合だけです
err-kind-symlink = シンボリックリンク
err-kind-submodule = サブモジュール
err-kind-directory = ディレクトリ
err-deleted-everywhere = { $path } はすべての側で削除されています。マージするものはありません
err-resolved-in-tree = { $path } の作業ツリーには競合マーカーが残っていないため、解決済みに見えます。解決済みにするか、マーカーを戻してマージし直してください
err-tree-not-text = { $path }: 作業ツリーのファイルがテキストではない ({ $detail }) ため、保存で上書きされる編集が含まれている可能性があります。競合マーカー付きのテキストに戻すか、手で解決してください
err-not-a-directory = { $path } はディレクトリではありません
err-mixed = diff には 2 つのファイルか 2 つのディレクトリを渡します: { $dir } はディレクトリで、{ $other } は違います
err-git-in-jj = { $path } は jj リポジトリで、triad は jj の方式でのみマージします。`jj resolve` で解決してください
err-git-in-jj-resolve = { $path } は jj の解決用ディレクトリで、triad は jj の方式でのみマージします。`jj resolve` で解決してください
err-no-markers = 競合マーカーがありません
err-unclosed = { $line } 行目: 競合が閉じていません
err-nested = { $line } 行目: 競合が別の競合の中で始まっています
err-out-of-order = { $line } 行目: マーカーの順序が正しくありません
err-no-separator = { $line } 行目: 競合が ======= の前に終わっています
err-bad-diff-line = { $line } 行目: diff の行が ' '、'-'、'+' のいずれでも始まっていません
err-marker-shape = { $line } 行目: 競合には 2 つ以上の面と、それより 1 つ少ないベースが必要ですが、面が { $sides }、ベースが { $bases } です
err-mixed-arity = { $line } 行目: { $expected } 面の競合の後に { $found } 面の競合があります
err-shape = マージには 2 つ以上の面と、それより 1 つ少ないベースが必要ですが、面が { $sides }、ベースが { $bases } です
err-journal-header = ジャーナルに完全なヘッダー行がありません
err-journal-line = ジャーナルの { $index } 行目を解析できません: { $detail }
err-journal-version = ジャーナルの形式は { $found } で、{ $expected } ではありません
err-journal-inputs = ジャーナルは別の入力用に書かれています
err-journal-alignment = ジャーナルはこれらの入力の別の配置用に書かれています (別のバージョンの triad によるものなど)
err-journal-command = ジャーナルのコマンド { $index } を適用できません: { $detail }
err-cannot-open-window = ウィンドウを開けません: { $detail }
err-io-not-found = そのようなファイルやディレクトリはありません
err-io-permission = アクセスが拒否されました
err-io-exists = ファイルは既に存在します
err-io-is-directory = ディレクトリです
err-io-not-directory = ディレクトリではありません
err-io-full = デバイスに空き容量がありません
err-io-read-only = ファイルシステムは読み取り専用です

## gpui-component's text

component-replace = 置換
component-replace-all = すべて置換
component-clear = クリア
