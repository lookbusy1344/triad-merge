# The windows' text in simplified Chinese. `en.ftl` is the reference: it says how counts and
# names work. Chinese has one plural form, so counts need no selector.

## About window

about-title = 关于 triad
about-description = 三方合并与双向对比
about-vcs-support = 为 Jujutsu 打造，全面支持 Git
about-version = 版本 { $version }
about-build-commit = 提交 { $commit }
about-build-commit-change = 提交 { $commit }，变更 { $change }

## Window parts

result-title = 结果
merge-toolbar = 合并工具
diff-toolbar = 对比工具
input-lines = 输入行
go-to-line-field = 行号
no-matches = 无匹配
close = 关闭
settings-errors = 配置文件有错误：{ $errors }

## Save, cancel and restore questions

prompt-save-unresolved = 带着未解决的冲突保存？
prompt-save-with-conflicts = 带冲突保存
prompt-keep-editing = 继续编辑
prompt-discard-detail = 放弃不会写入任何内容，合并在 git 或 jj 中仍未解决。
prompt-discard = 放弃
prompt-cannot-write = 无法写入输出
prompt-cannot-write-copy = 无法写入副本
prompt-cannot-record = 仓库未记录此次保存
prompt-no-output = 此窗口没有输出文件。
prompt-quit = 退出
prompt-try-again = 重试
prompt-restore-session = 恢复上一个会话？
prompt-restore-detail = 这些文件上的一个会话在合并解决之前结束了。可以恢复其选择和编辑。
prompt-restore = 恢复
prompt-start-over = 重新开始
prompt-cannot-restore = 无法恢复会话
prompt-save-changes = 保存更改？
prompt-cannot-save = 无法保存文件
prompt-ok = 好
prompt-changed-detail = 重新载入并丢弃你的编辑，或保留你的版本稍后覆盖保存。
prompt-reload = 重新载入
prompt-output-changed-detail = 合并打开后，另一个程序修改了它。保存并退出会用合并结果替换这些修改。
prompt-keep-mine = 保留我的
prompt-changed-on-disk = { $name } 在磁盘上已更改。
prompt-descendants-changed = 保存会改变 { $count } 个后代的冲突。
prompt-unresolved-detail = { $count } 个冲突保留其标记，git 或 jj 会认为合并未解决。
prompt-unresolved-detail-git = { $count } 个冲突保留其标记，git 或 jj 会认为合并未解决。随后 git mergetool 会放回它自己的文件副本；再次运行它即可恢复此会话。

## Counts

count-commits = { $count } 个提交
count-files = { $count } 个文件
count-versions = { $count } 个版本
count-hunks = { $count } 个差异块
count-differences = { $count } 处差异
count-conflicts = { $count } 个冲突
count-unchanged-lines = { $count } 行未更改
count-duplicated-blocks = { $count } 个重复块

## Conflicted commits (stack window)

no-paths = 无
stack-title = 有冲突的提交
stack-empty = 没有有冲突的提交。
refresh = 刷新
stack-preview = 预览保存
preview-running = 正在计算保存的效果…
working-copy = 工作副本
no-description = （未设置描述）
commit-tooltip =
    变更 ID：{ $change }
    提交 ID：{ $commit }
    日期：{ $date }
inherited = （继承）
stack-resolved = 已在 { $change } 中解决 { $path }。
stack-saved-conflicts = 已在 { $change } 中带冲突保存 { $path }。
stack-error = { $change } 中 { $path } 的合并以错误结束。
stack-inherited = { $change } 中的 { $path } 继承了 { $origin } 的冲突。先在 { $origin } 中解决，这里的冲突也会随之消失；先在这里保存的解决很可能会再次冲突。
preview-resolves = 保存会解决 { $change } 中的该路径。
preview-keeps = 保存后 { $change } 中仍有冲突。
preview-no-rebase = 没有后代被变基。

## Status icons

status-added = 已添加
status-modified = 已修改
status-deleted = 已删除
status-moved = 已移动
status-copied = 已复制
status-conflicted = 有冲突

## Changed files (directory window)

dirs-title = 已更改的文件
dirs-empty = 没有文件不同。
dirs-against = { $files }，对比 { $rev }
file-added = 已添加
file-deleted = 已删除
file-modified = 已修改
file-renamed = 重命名自 { $from }
file-copied = 复制自 { $from }
file-from = 来自 { $from }
file-undecodable = 无法解码
file-binary = 二进制
file-special = 特殊文件
file-label = { $path } { $change }
file-label-why = { $path } { $change }，{ $why }
at-operation = 操作 { $operation } 时的 @
no-file = { $name }（无文件）

## History window

history-title = { $path } 的历史
history-empty = 没有版本。
version-conflict = （冲突）
evolog-title = { $change } 的版本
interdiff-title = 自 { $short } 以来的 { $change }
dirs-since = { $files }，自 { $short } 以来
interdiff-rebased = { $short } 时的 { $change }（已变基）
interdiff-version = { $short } 时的 { $change }
evolog-no-difference = 与当前版本没有差异。
version-absent = （不存在）
version-other = （其他）

## Absorb window

absorb-title = 吸收
absorb-empty = 没有可吸收的内容。
absorb-apply = 应用
absorb-keep = 工作副本
absorb-note = 已将 { $hunks } 吸收到 { $commits } 中。
absorb-stale = 工作副本已过时：请运行 jj workspace update-stale。
absorb-skipped = 已跳过 { $path }：{ $reason }

## Open dialog

open-title-merge = 打开合并
open-title-diff = 打开对比
mode-merge = 合并
mode-diff = 对比
start-merge = 合并
start-diff = 对比
slot-base = 基础
slot-left = 左侧
slot-right = 右侧
slot-output = 输出
choose-base = 选择基础文件
choose-left = 选择左侧文件
choose-right = 选择右侧文件
choose-output = 选择输出文件
missing-slot = 请为“{ $slot }”选择文件。

## Menus

menu-settings = 设置…
menu-save = 保存
menu-save-and-exit = 保存并退出
menu-save-copy-as = 副本另存为…
menu-cancel = 取消
menu-edit = 编辑
menu-merge = 合并
menu-go = 前往
menu-view = 视图
menu-undo = 撤销
menu-redo = 重做
menu-find = 查找…
menu-go-to-line = 前往行…
menu-copy-as-patch = 复制为补丁
menu-copy-difference-as-patch = 将差异复制为补丁
menu-toggle-left = 切换左侧
menu-toggle-base = 切换基础
menu-toggle-right = 切换右侧
menu-clear-choice = 清除选择
menu-pick-selected-lines = 选取所选行
menu-take-base = 采用基础
menu-take-left = 采用左侧
menu-take-right = 采用右侧
menu-take-left-then-right = 先左后右
menu-take-right-then-left = 先右后左
menu-take-sorted-union = 采用排序并集
menu-take-left-all-unresolved = 所有未解决处采用左侧
menu-take-right-all-unresolved = 所有未解决处采用右侧
menu-take-whole-left = 采用整个左侧文件
menu-take-whole-right = 采用整个右侧文件
menu-carry-moved-edit = 将编辑带到移动的块
menu-drop-duplicate = 删除重复
menu-next-difference = 下一处差异
menu-previous-difference = 上一处差异
menu-next-conflict = 下一个冲突
menu-previous-conflict = 上一个冲突
menu-go-to-move-end = 前往移动的另一端
menu-base-pane = 基础窗格
menu-blame = 追溯
menu-syntax = 语法高亮
menu-soft-wrap = 自动换行
menu-whitespace = 显示空白字符
menu-collapse-unchanged = 折叠未更改
menu-unified = 统一视图
menu-scroll-sync = 同步结果滚动

## Toolbar

tool-label-difference = 差异
tool-label-conflict = 冲突
tool-label-left = 左侧
tool-label-right = 右侧
more = 更多
more-menu = 更多选择
more-base = 基础
more-left-then-right = 先左后右
more-right-then-left = 先右后左
more-none = 无
tool-previous-difference = 上一处差异
tool-next-difference = 下一处差异
tool-previous-conflict = 上一个冲突
tool-next-conflict = 下一个冲突
tool-left = 在当前区域切换左侧
tool-base = 在当前区域切换基础
tool-right = 在当前区域切换右侧
tool-left-then-right = 在当前区域采用先左后右
tool-right-then-left = 在当前区域采用先右后左
tool-clear = 清除当前区域的选择
tool-undo = 撤销
tool-redo = 重做
tool-base-pane = 显示或隐藏基础窗格
tool-find = 查找
tool-go-to-line = 前往结果中的某一行
tool-syntax = 打开或关闭语法颜色
tool-soft-wrap = 打开或关闭自动换行
tool-whitespace = 显示或隐藏空白字符

## Conflict badge and pick targets

pick-label = { $title }，第 { $region }/{ $regions } 个区域
pick-label-unresolved = { $title }，第 { $region }/{ $regions } 个区域，未解决
pick-label-inherited = { $title }，第 { $region }/{ $regions } 个区域，未解决，继承
inherited-pill = 继承
line-pick-label = { $title }，第 { $line } 行
badge-conflict = 冲突 { $number }/{ $count }
badge-sides = { $kept }/{ $sides } 方
badge-all-resolved = 全部已解决
badge-resolved-one = 冲突已解决
badge-resolved-all = { $count } 个全部已解决
badge-unresolved-of = { $unresolved }/{ $count } 未解决

## Panes

role-left = 左侧
role-right = 右侧
role-base = 基础
role-side = 第 { $number } 方
role-base-of = 基础 { $number }
role-result = 结果
cell-line = { $title }，第 { $line } 行：{ $text }
cell-no-line = { $title }，无此行
cell-no-line-deleted = { $title }，无此行：{ $deleted }
placeholder-nothing = 未选择（丢弃 { $count } 行）
placeholder-deleted = 已删除 { $count } 行（{ $roles }）
lines-deleted = 已删除 { $count } 行
previous-match = { $title } 中的上一个匹配
next-match = { $title } 中的下一个匹配
match-count = { $number }/{ $count }
go-to-line-prompt = 前往行（1–{ $last }）：
go-to-line-current = 当前第 { $line } 行，共 { $last } 行
go-to-line-target = 前往第 { $line } 行
jj-diff-banner = 这些是来自 jj diff --tool 的临时副本。窗口关闭后，此处的编辑将丢失。
snapshot-failed-banner = 已保存，但 jj 未记录此次保存：{ $error }
output-changed-banner = { $name } 在合并打开后于磁盘上被修改。保存并退出会替换它。
copy-to-right = 将此差异复制到右侧
copy-to-left = 将此差异复制到左侧
initial-base = 基
initial-left = 左
initial-right = 右
moved-to-base = 移动的块：前往其另一端，基础第 { $line } 行
moved-to-left = 移动的块：前往其另一端，左侧第 { $line } 行
moved-to-right = 移动的块：前往其另一端，右侧第 { $line } 行
diff-pane-left = 左侧：{ $name }
diff-pane-right = 右侧：{ $name }
diff-pane-unified = 统一视图：{ $left } 对比 { $right }

## Status bar

repo-git = git 仓库
repo-jj = jj 仓库
repo-none = 无仓库
mode-git = git 合并
mode-jj = jj 合并
mode-git-word = git 按词合并
mode-jj-word = jj 按词合并
encoding-utf8-bom = UTF-8 带 BOM
encoding-utf16le-bom = UTF-16 LE 带 BOM
encoding-utf16be-bom = UTF-16 BE 带 BOM
inputs-differ = { $encoding }（输入不同）
word-merged = { $count } 处按词合并
unresolved = { $count } 个未解决
mixed-endings = 换行符混用
compare-whitespace = 空白有意义
compare-line-endings = 忽略换行符
compare-trailing = 忽略行尾空白
compare-amount = 忽略空白数量
compare-all = 忽略所有空白
case-ignored = 忽略大小写
patterns-ignored = 忽略模式
blank-lines-ignored = 忽略空行
diverge-whitespace-jj = 空白比较方式与 jj 不同
diverge-case-git = 忽略大小写，与 git 不同
diverge-case-jj = 忽略大小写，与 jj 不同
diverge-patterns-git = 忽略模式，与 git 不同
diverge-patterns-jj = 忽略模式，与 jj 不同
diverge-same-change-git = 保留相同更改，与 git 不同
diverge-word-git = 按词合并，与 git 不同
diverge-deep-git = 冲突内部拆分，与 git 不同
diverge-deep-jj = 冲突内部拆分，与 jj 不同
diverge-syntax-git = 保留语法边界，与 git 不同
diverge-syntax-jj = 保留语法边界，与 jj 不同
copy-saved =
    { $count ->
        [0] 副本已保存，已解决
       *[other] 副本已保存，含 { $count } 个冲突
    }
saved =
    { $count ->
        [0] 已保存，已解决
       *[other] 已保存，含 { $count } 个冲突
    }

## Result pane's right-click menu

choice-none = 无
choice-base = 基础
choice-left = 左侧
choice-right = 右侧
choice-left-then-right = 左侧 + 右侧
choice-right-then-left = 右侧 + 左侧
action-sorted-union = 排序并集
suggested = 建议：{ $action }
suggested-carry = 建议：将{ $from }的编辑带到第 { $line } 行
suggested-drop = 建议：删除第 { $line } 行的重复
edit-cut = 剪切
edit-copy = 复制
edit-paste = 粘贴
edit-select-all = 全选

## Error details

err-cannot-read = 无法读取 { $path }：{ $detail }
err-cannot-read-plain = 无法读取 { $path }
err-cannot-write = 无法写入 { $path }：{ $detail }
err-cannot-save = 无法保存 { $path }：{ $detail }
err-cannot-find = 找不到 { $path }：{ $detail }
err-in = { $path }：{ $detail }
err-at-rev = { $rev } 中的 { $path }：{ $detail }
err-not-recorded = { $path } 已写入，但仓库未记录此次保存：{ $detail }
err-save-writing = 正在保存合并
err-no-output = 合并没有可写入的输出
err-save-running = 正在保存
err-encode = { $encoding } 无法容纳第 { $line } 行的 { $character }（U+{ $code }）
err-binary = 二进制文件：前 { $bytes } 个字节中有 NUL 字节
err-invalid-utf8 = 第 { $offset } 个字节处的 UTF-8 无效
err-invalid-utf16 = 第 { $offset } 个字节处有未配对的 UTF-16 代理项
err-odd-utf16 = 字节数为奇数的 UTF-16 文件
err-not-a-repository = { $path } 不在 git 或 jj 仓库中
err-not-utf8-path = { $path } 不是 UTF-8 路径
err-no-working-copy = { $path } 没有工作副本
err-outside-repository = { $path } 在仓库之外
err-stale-file = 磁盘上的 { $path } 不是工作副本提交中的版本；请运行 jj workspace update-stale 后重试
err-stale-workspace = { $path } 中的工作副本已过时；请运行 jj workspace update-stale 后重试
err-unknown-revision = 未知修订 { $rev }
err-ambiguous-revision = 修订 { $rev } 有歧义
err-conflicted-at = { $path } 在 { $rev } 中有冲突
err-no-index = { $name } 没有索引
err-immutable = 提交 { $rev } 不可变
err-not-conflicted-at = { $path } 在 { $rev } 中没有冲突
err-bad-config = { $key } 的值 { $value } 未知
err-backend = { $context }：{ $detail }
err-concurrent-operations = 仓库有 { $count } 个并发操作；任何 jj 命令都会协调它们
err-not-a-file-conflict = { $path } 不是文件之间的冲突
err-unknown-operation = 没有操作 { $op }：{ $reason }
err-no-working-copy-at = 工作区在操作 { $op } 时没有工作副本
err-no-hunk = 没有差异块 { $hunk }；共有 { $count } 个
err-not-a-destination = --into 中的 { $rev } 不是工作副本的可变祖先
err-repository-moved = 自操作 { $op } 以来仓库已变更；请重新列出差异块
err-conflict-changed = { $change } 中的 { $path } 在 triad 读取后已变更；未写入任何内容
err-sides-changed = { $path }：jj 把一个 { $before } 方的冲突读回为 { $after } 方
err-jj-config-load = 无法读取 jj 配置 { $path }：{ $detail }
err-jj-config-id = { $path }：不是 jj 配置 ID
err-jj-config-value = jj 配置中的 { $key }：{ $detail }
err-jj-config-value-in = { $path }：jj 配置中的 { $key }：{ $detail }
err-jj-config-get = 读取 jj 配置值
err-jj-config-set = 设置 jj 配置值
err-jj-config-find = 查找仓库或工作区的 jj 配置
err-auto-track = `snapshot.auto-track = { $text }` 不是 fileset
err-git-ignores = 读取 git 的忽略文件
err-jj-workspace = 加载 jj 工作区
err-needs-jj = { $command } 需要 jj 仓库
err-not-conflicted = { $path } 没有冲突
err-term-not-text = { $path }：{ $term } 不是文本：{ $detail }
err-term-not-a-file = { $path }：{ $term } 是{ $kind }；只能解决文本冲突
err-kind-symlink = 符号链接
err-kind-submodule = 子模块
err-kind-directory = 目录
err-deleted-everywhere = { $path } 在每一方都已删除；没有可合并的内容
err-resolved-in-tree = { $path } 在工作树中已没有冲突标记，看起来已解决；请将其标记为已解决，或放回标记以重新合并
err-tree-not-text = { $path }：工作树中的文件不是文本（{ $detail }），可能含有保存会覆盖的编辑；请放回带冲突标记的文本，或手动解决
err-not-a-directory = { $path } 不是目录
err-mixed = diff 需要两个文件或两个目录：{ $dir } 是目录，{ $other } 不是
err-git-in-jj = { $path } 是 jj 仓库，triad 在此只按 jj 的语义合并；请用 `jj resolve` 解决
err-git-in-jj-resolve = { $path } 是 jj 的解决目录，triad 在此只按 jj 的语义合并；请用 `jj resolve` 解决
err-no-markers = 没有冲突标记
err-unclosed = 第 { $line } 行：冲突未闭合
err-nested = 第 { $line } 行：冲突在另一个冲突内部开始
err-out-of-order = 第 { $line } 行：标记顺序错误
err-no-separator = 第 { $line } 行：冲突在其 ======= 之前结束
err-bad-diff-line = 第 { $line } 行：diff 行不以 ' '、'-' 或 '+' 开头
err-marker-shape = 第 { $line } 行：冲突至少需要两方，且基础比方数少一个，实际为 { $sides } 方、{ $bases } 个基础
err-mixed-arity = 第 { $line } 行：在 { $expected } 方的冲突之后出现 { $found } 方的冲突
err-shape = 合并至少需要两方，且基础比方数少一个，实际为 { $sides } 方、{ $bases } 个基础
err-journal-header = 日志没有完整的标头行
err-journal-line = 无法解析日志第 { $index } 行：{ $detail }
err-journal-version = 日志格式为 { $found }，而不是 { $expected }
err-journal-inputs = 日志是为其他输入写的
err-journal-alignment = 日志是为这些输入的另一种对齐写的，例如来自另一个版本的 triad
err-journal-command = 无法应用日志中的命令 { $index }：{ $detail }
err-cannot-open-window = 无法打开窗口：{ $detail }
err-io-not-found = 没有那个文件或目录
err-io-permission = 权限被拒绝
err-io-exists = 文件已存在
err-io-is-directory = 这是一个目录
err-io-not-directory = 不是目录
err-io-full = 设备上没有剩余空间
err-io-read-only = 文件系统为只读

## gpui-component's text

component-replace = 替换
component-replace-all = 全部替换
component-clear = 清除
