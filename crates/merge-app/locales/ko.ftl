# The windows' text in Korean. `en.ftl` is the reference: it says how counts and names work.
# Korean has one plural form, so counts need no selector.

## About window

about-title = triad 정보
about-description = 3방향 병합과 2방향 비교
about-vcs-support = Jujutsu용으로 만들었으며 Git을 완전히 지원
about-version = 버전 { $version }
about-build-commit = 커밋 { $commit }
about-build-commit-change = 커밋 { $commit }, 변경 { $change }

## Window parts

result-title = 결과
merge-toolbar = 병합 도구
diff-toolbar = 비교 도구
input-lines = 입력 줄
go-to-line-field = 줄 번호
no-matches = 일치 항목 없음
close = 닫기
settings-errors = 설정 파일에 오류가 있습니다: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = 해결되지 않은 충돌이 있는 채로 저장할까요?
prompt-save-with-conflicts = 충돌과 함께 저장
prompt-keep-editing = 계속 편집
prompt-discard-detail = 버리면 아무것도 쓰지 않으며, 병합은 git 또는 jj에서 해결되지 않은 상태로 남습니다.
prompt-discard = 버리기
prompt-cannot-write = 출력을 쓸 수 없습니다
prompt-cannot-write-copy = 사본을 쓸 수 없습니다
prompt-cannot-record = 저장소가 저장을 기록하지 않았습니다
prompt-no-output = 이 창에는 출력 파일이 없습니다.
prompt-quit = 종료
prompt-try-again = 다시 시도
prompt-restore-session = 이전 세션을 복원할까요?
prompt-restore-detail = 이 파일들의 세션이 병합이 해결되기 전에 끝났습니다. 그 선택과 편집을 되돌릴 수 있습니다.
prompt-restore = 복원
prompt-start-over = 처음부터 시작
prompt-cannot-restore = 세션을 복원할 수 없습니다
prompt-save-changes = 변경 사항을 저장할까요?
prompt-cannot-save = 파일을 저장할 수 없습니다
prompt-ok = 확인
prompt-changed-detail = 다시 불러와 편집을 잃거나, 내 버전을 유지하고 나중에 덮어써 저장하세요.
prompt-reload = 다시 불러오기
prompt-overwrite = 덮어쓰기
prompt-output-changed-detail = 병합을 연 뒤 다른 프로그램이 변경했습니다. 저장 후 종료하면 그 변경이 병합 결과로 바뀝니다.
prompt-keep-mine = 내 버전 유지
prompt-changed-on-disk = { $name } 파일이 디스크에서 변경되었습니다.
prompt-descendants-changed = 저장하면 후손 { $count }개의 충돌이 바뀝니다.
prompt-unresolved-detail = 충돌 { $count }개가 표시를 유지하며, git 또는 jj는 병합을 해결되지 않은 것으로 봅니다.
prompt-unresolved-detail-git = 충돌 { $count }개가 표시를 유지하며, git 또는 jj는 병합을 해결되지 않은 것으로 봅니다. 그 후 git mergetool이 자신의 파일 사본을 되돌려 놓으니, 이 세션을 복원하려면 다시 실행하세요.

## Counts

count-commits = 커밋 { $count }개
count-files = 파일 { $count }개
count-versions = 버전 { $count }개
count-hunks = 헝크 { $count }개
count-differences = 차이 { $count }개
count-conflicts = 충돌 { $count }개
count-unchanged-lines = 변경되지 않은 줄 { $count }개
count-duplicated-blocks = 중복 블록 { $count }개

## Conflicted commits (stack window)

no-paths = 없음
stack-title = 충돌이 있는 커밋
stack-empty = 충돌이 있는 커밋이 없습니다.
refresh = 새로 고침
stack-preview = 저장 미리 보기
preview-running = 저장 결과를 계산하는 중…
working-copy = 작업 사본
no-description = (설명 없음)
commit-tooltip =
    변경 ID: { $change }
    커밋 ID: { $commit }
    날짜: { $date }
inherited = (상속됨)
stack-resolved = { $change }에서 { $path }을(를) 해결했습니다.
stack-saved-conflicts = { $change }에 { $path }을(를) 충돌과 함께 저장했습니다.
stack-error = { $change }에서 { $path }의 병합이 오류로 끝났습니다.
preview-resolves = 저장하면 { $change }에서 이 경로가 해결됩니다.
preview-keeps = 저장 후에도 { $change }에 충돌이 남습니다.
preview-no-rebase = 리베이스되는 후손이 없습니다.

## Status icons

status-added = 추가됨
status-modified = 수정됨
status-deleted = 삭제됨
status-moved = 이동됨
status-copied = 복사됨
status-conflicted = 충돌

## Changed files (directory window)

dirs-title = 변경된 파일
dirs-empty = 다른 파일이 없습니다.
dirs-against = { $files }, { $rev }과(와) 비교
file-added = 추가됨
file-deleted = 삭제됨
file-modified = 수정됨
file-renamed = { $from }에서 이름 변경
file-copied = { $from }에서 복사
file-from = { $from }에서
file-undecodable = 디코딩 불가
file-binary = 바이너리
file-special = 특수 파일
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = 작업 { $operation } 시점의 @
no-file = { $name } (파일 없음)

## History window

history-title = { $path } 기록
history-empty = 버전이 없습니다.
version-conflict = (충돌)
version-absent = (없음)
version-other = (기타)

## Absorb window

absorb-title = 흡수
absorb-empty = 흡수할 것이 없습니다.
absorb-apply = 적용
absorb-keep = 작업 사본
absorb-note = { $hunks }을(를) { $commits }에 흡수했습니다.
absorb-stale = 작업 사본이 오래되었습니다: jj workspace update-stale을 실행하세요.
absorb-skipped = { $path } 건너뜀: { $reason }

## Open dialog

open-title-merge = 병합 열기
open-title-diff = 비교 열기
mode-merge = 병합
mode-diff = 비교
start-merge = 병합
start-diff = 비교
slot-base = 기준
slot-left = 왼쪽
slot-right = 오른쪽
slot-output = 출력
choose-base = 기준 파일 선택
choose-left = 왼쪽 파일 선택
choose-right = 오른쪽 파일 선택
choose-output = 출력 파일 선택
missing-slot = ‘{ $slot }’에 쓸 파일을 선택하세요.

## Menus

menu-settings = 설정…
menu-save = 저장
menu-save-and-exit = 저장 후 종료
menu-save-copy-as = 다른 이름으로 사본 저장…
menu-cancel = 취소
menu-edit = 편집
menu-merge = 병합
menu-go = 이동
menu-view = 보기
menu-undo = 실행 취소
menu-redo = 다시 실행
menu-find = 찾기…
menu-go-to-line = 줄로 이동…
menu-toggle-left = 왼쪽 전환
menu-toggle-base = 기준 전환
menu-toggle-right = 오른쪽 전환
menu-clear-choice = 선택 지우기
menu-take-base = 기준 채택
menu-take-left = 왼쪽 채택
menu-take-right = 오른쪽 채택
menu-take-left-then-right = 왼쪽 다음 오른쪽 채택
menu-take-right-then-left = 오른쪽 다음 왼쪽 채택
menu-take-sorted-union = 정렬된 합집합 채택
menu-take-left-all-unresolved = 해결되지 않은 모든 곳에 왼쪽 채택
menu-take-right-all-unresolved = 해결되지 않은 모든 곳에 오른쪽 채택
menu-take-whole-left = 왼쪽 파일 전체 채택
menu-take-whole-right = 오른쪽 파일 전체 채택
menu-carry-moved-edit = 편집을 이동된 블록으로 옮기기
menu-drop-duplicate = 중복 제거
menu-next-difference = 다음 차이
menu-previous-difference = 이전 차이
menu-next-conflict = 다음 충돌
menu-previous-conflict = 이전 충돌
menu-go-to-move-end = 이동의 다른 끝으로
menu-base-pane = 기준 창
menu-blame = 출처
menu-syntax = 구문 강조
menu-soft-wrap = 자동 줄 바꿈
menu-whitespace = 공백 표시
menu-collapse-unchanged = 변경되지 않은 부분 접기
menu-scroll-sync = 결과 스크롤 동기화

## Toolbar

tool-label-difference = 차이
tool-label-conflict = 충돌
tool-label-left = 왼쪽
tool-label-right = 오른쪽
more = 더 보기
more-menu = 다른 선택
more-base = 기준
more-left-then-right = 왼쪽 다음 오른쪽
more-right-then-left = 오른쪽 다음 왼쪽
more-none = 없음
tool-previous-difference = 이전 차이
tool-next-difference = 다음 차이
tool-previous-conflict = 이전 충돌
tool-next-conflict = 다음 충돌
tool-left = 현재 영역에서 왼쪽 전환
tool-base = 현재 영역에서 기준 전환
tool-right = 현재 영역에서 오른쪽 전환
tool-left-then-right = 현재 영역에서 왼쪽 다음 오른쪽 채택
tool-right-then-left = 현재 영역에서 오른쪽 다음 왼쪽 채택
tool-clear = 현재 영역의 선택 지우기
tool-undo = 실행 취소
tool-redo = 다시 실행
tool-base-pane = 기준 창 표시 또는 숨기기
tool-find = 찾기
tool-go-to-line = 결과의 줄로 이동
tool-syntax = 구문 색상 켜기 또는 끄기
tool-soft-wrap = 자동 줄 바꿈 켜기 또는 끄기
tool-whitespace = 공백 표시 또는 숨기기

## Conflict badge and pick targets

pick-label = { $title }, 영역 { $region }/{ $regions }
pick-label-unresolved = { $title }, 영역 { $region }/{ $regions }, 해결되지 않음
badge-conflict = 충돌 { $number }/{ $count }
badge-sides = { $kept }/{ $sides }개 쪽
badge-all-resolved = 모두 해결됨
badge-resolved-one = 충돌 해결됨
badge-resolved-all = { $count }개 모두 해결됨
badge-unresolved-of = { $unresolved }/{ $count }개 해결되지 않음

## Panes

role-left = 왼쪽
role-right = 오른쪽
role-base = 기준
role-side = 쪽 { $number }
role-base-of = 기준 { $number }
role-result = 결과
cell-line = { $title }, { $line }번째 줄: { $text }
cell-no-line = { $title }, 줄 없음
cell-no-line-deleted = { $title }, 줄 없음: { $deleted }
placeholder-nothing = 선택 없음({ $count }줄 버림)
placeholder-deleted = { $count }줄 삭제됨({ $roles })
lines-deleted = { $count }줄 삭제됨
previous-match = { $title }의 이전 일치 항목
next-match = { $title }의 다음 일치 항목
match-count = { $number }/{ $count }
go-to-line-prompt = 줄로 이동(1–{ $last }):
go-to-line-current = 현재 줄 { $line }/{ $last }
go-to-line-target = { $line }번째 줄로 이동
jj-diff-banner = jj diff --tool이 만든 임시 사본입니다. 창을 닫으면 여기서 한 편집은 사라집니다.
output-changed-banner = 병합을 연 뒤 디스크에서 { $name }이(가) 변경되었습니다. 저장 후 종료하면 덮어씁니다.
copy-to-right = 이 차이를 오른쪽으로 복사
copy-to-left = 이 차이를 왼쪽으로 복사
initial-base = B
initial-left = L
initial-right = R
moved-to-base = 이동된 블록: 다른 끝, 기준 { $line }번째 줄로
moved-to-left = 이동된 블록: 다른 끝, 왼쪽 { $line }번째 줄로
moved-to-right = 이동된 블록: 다른 끝, 오른쪽 { $line }번째 줄로
diff-pane-left = 왼쪽: { $name }
diff-pane-right = 오른쪽: { $name }

## Status bar

repo-git = git 저장소
repo-jj = jj 저장소
repo-none = 저장소 없음
mode-git = git 병합
mode-jj = jj 병합
mode-git-word = git 단어 단위 병합
mode-jj-word = jj 단어 단위 병합
encoding-utf8-bom = UTF-8 BOM 포함
inputs-differ = { $encoding }(입력이 다름)
word-merged = { $count }개 단어 단위로 병합
unresolved = { $count }개 해결되지 않음
mixed-endings = 줄 끝 혼용
compare-whitespace = 공백 구분
compare-line-endings = 줄 끝 무시
compare-trailing = 줄 끝 공백 무시
compare-amount = 공백 양 무시
compare-all = 모든 공백 무시
case-ignored = 대소문자 무시
patterns-ignored = 패턴 무시
blank-lines-ignored = 빈 줄 무시
diverge-whitespace-jj = 공백 비교가 jj와 다름
diverge-case-git = 대소문자 무시, git과 다름
diverge-case-jj = 대소문자 무시, jj와 다름
diverge-patterns-git = 패턴 무시, git과 다름
diverge-patterns-jj = 패턴 무시, jj와 다름
diverge-same-change-git = 같은 변경 유지, git과 다름
diverge-word-git = 단어 단위 병합, git과 다름
diverge-deep-git = 충돌 내부 분할, git과 다름
diverge-deep-jj = 충돌 내부 분할, jj와 다름
diverge-syntax-git = 구문 경계 유지, git과 다름
diverge-syntax-jj = 구문 경계 유지, jj와 다름
copy-saved =
    { $count ->
        [0] 사본 저장됨, 해결됨
       *[other] 사본 저장됨, 충돌 { $count }개
    }

## Result pane's right-click menu

choice-none = 없음
choice-base = 기준
choice-left = 왼쪽
choice-right = 오른쪽
choice-left-then-right = 왼쪽 + 오른쪽
choice-right-then-left = 오른쪽 + 왼쪽
action-sorted-union = 정렬된 합집합
suggested = 제안: { $action }
suggested-carry = 제안: { $from }의 편집을 { $line }번째 줄로 옮기기
suggested-drop = 제안: { $line }번째 줄의 중복 제거
edit-cut = 잘라내기
edit-copy = 복사
edit-paste = 붙여넣기
edit-select-all = 모두 선택

## Error details

err-cannot-read = { $path }을(를) 읽을 수 없습니다: { $detail }
err-cannot-read-plain = { $path }을(를) 읽을 수 없습니다
err-cannot-write = { $path }에 쓸 수 없습니다: { $detail }
err-cannot-save = { $path }을(를) 저장할 수 없습니다: { $detail }
err-cannot-find = { $path }을(를) 찾을 수 없습니다: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $rev }의 { $path }: { $detail }
err-not-recorded = { $path }은(는) 기록되었지만 저장소가 저장을 기록하지 않았습니다: { $detail }
err-save-writing = 저장이 병합을 쓰는 중입니다
err-no-output = 병합에 쓸 출력이 없습니다
err-save-running = 저장이 진행 중입니다
err-encode = { $encoding }은(는) { $line }번째 줄의 { $character } (U+{ $code })을(를) 담을 수 없습니다
err-binary = 바이너리 파일: 처음 { $bytes }바이트 안에 NUL 바이트가 있습니다
err-invalid-utf8 = { $offset }번째 바이트에 잘못된 UTF-8이 있습니다
err-invalid-utf16 = { $offset }번째 바이트에 짝이 없는 UTF-16 서로게이트가 있습니다
err-odd-utf16 = 바이트 수가 홀수인 UTF-16 파일입니다
err-not-a-repository = { $path }은(는) git 또는 jj 저장소 안에 있지 않습니다
err-not-utf8-path = { $path }은(는) UTF-8 경로가 아닙니다
err-no-working-copy = { $path }에 작업 사본이 없습니다
err-outside-repository = { $path }은(는) 저장소 밖에 있습니다
err-stale-file = 디스크의 { $path }은(는) 작업 사본 커밋의 것이 아닙니다. jj workspace update-stale을 실행한 뒤 다시 시도하세요
err-stale-workspace = { $path }의 작업 사본이 오래되었습니다. jj workspace update-stale을 실행한 뒤 다시 시도하세요
err-unknown-revision = 알 수 없는 리비전 { $rev }
err-ambiguous-revision = 리비전 { $rev }이(가) 모호합니다
err-conflicted-at = { $path }은(는) { $rev }에서 충돌합니다
err-no-index = { $name }에 인덱스가 없습니다
err-immutable = 커밋 { $rev }은(는) 변경할 수 없습니다
err-not-conflicted-at = { $path }은(는) { $rev }에서 충돌하지 않습니다
err-bad-config = { $key }의 값 { $value }을(를) 알 수 없습니다
err-backend = { $context }: { $detail }
err-concurrent-operations = 저장소에 동시 작업이 { $count }개 있습니다. 아무 jj 명령이나 이를 조정합니다
err-not-a-file-conflict = { $path }은(는) 파일 간 충돌이 아닙니다
err-unknown-operation = 작업 { $op }이(가) 없습니다: { $reason }
err-no-working-copy-at = 작업 { $op } 시점에 작업 공간에 작업 사본이 없습니다
err-no-hunk = 헝크 { $hunk }이(가) 없습니다. 헝크는 { $count }개입니다
err-not-a-destination = --into의 { $rev }은(는) 작업 사본의 변경 가능한 조상이 아닙니다
err-repository-moved = 작업 { $op } 이후 저장소가 바뀌었습니다. 헝크를 다시 나열하세요
err-conflict-changed = { $change }의 { $path }이(가) triad가 읽은 뒤 바뀌었습니다. 아무것도 쓰지 않습니다
err-sides-changed = { $path }: jj가 { $before }면 충돌을 { $after }면 충돌로 다시 읽었습니다
err-jj-config-load = jj 설정 { $path }을(를) 읽을 수 없습니다: { $detail }
err-jj-config-id = { $path }: jj 설정 ID가 아닙니다
err-jj-config-value = jj 설정의 { $key }: { $detail }
err-jj-config-value-in = { $path }: jj 설정의 { $key }: { $detail }
err-jj-config-get = jj 설정 값 읽기
err-jj-config-set = jj 설정 값 쓰기
err-jj-config-find = 저장소 또는 작업 공간의 jj 설정 찾기
err-auto-track = `snapshot.auto-track = { $text }`은(는) fileset이 아닙니다
err-git-ignores = git 무시 파일 읽기
err-jj-workspace = jj 작업 공간 불러오기
err-needs-jj = { $command }에는 jj 저장소가 필요합니다
err-not-conflicted = { $path }은(는) 충돌하지 않습니다
err-term-not-text = { $path }: { $term }은(는) 텍스트가 아닙니다: { $detail }
err-term-not-a-file = { $path }: { $term }은(는) { $kind }입니다. 텍스트 충돌만 해결할 수 있습니다
err-kind-symlink = 심볼릭 링크
err-kind-submodule = 서브모듈
err-kind-directory = 디렉터리
err-deleted-everywhere = { $path }은(는) 모든 쪽에서 삭제되었습니다. 병합할 것이 없습니다
err-resolved-in-tree = { $path }의 작업 트리에 충돌 표시가 남아 있지 않아 해결된 것으로 보입니다. 해결됨으로 표시하거나, 표시를 되돌려 다시 병합하세요
err-tree-not-text = { $path }: 작업 트리의 파일이 텍스트가 아니므로({ $detail }) 저장하면 덮어쓸 편집이 들어 있을 수 있습니다. 충돌 표시가 있는 텍스트로 되돌리거나 직접 해결하세요
err-temporary = 결과용 임시 파일을 만들 수 없습니다: { $detail }
err-not-a-directory = { $path }은(는) 디렉터리가 아닙니다
err-mixed = diff는 파일 두 개 또는 디렉터리 두 개를 받습니다: { $dir }은(는) 디렉터리이고 { $other }은(는) 아닙니다
err-git-in-jj = { $path }은(는) jj 저장소이며, triad는 여기서 jj 방식으로만 병합합니다. `jj resolve`로 해결하세요
err-git-in-jj-resolve = { $path }은(는) jj의 해결용 디렉터리이며, triad는 여기서 jj 방식으로만 병합합니다. `jj resolve`로 해결하세요
err-no-markers = 충돌 표시가 없습니다
err-unclosed = { $line }번째 줄: 충돌이 닫히지 않았습니다
err-nested = { $line }번째 줄: 충돌이 다른 충돌 안에서 시작합니다
err-out-of-order = { $line }번째 줄: 표시의 순서가 맞지 않습니다
err-no-separator = { $line }번째 줄: 충돌이 ======= 앞에서 끝납니다
err-bad-diff-line = { $line }번째 줄: diff 줄이 ' ', '-', '+' 중 어느 것으로도 시작하지 않습니다
err-marker-shape = { $line }번째 줄: 충돌에는 둘 이상의 면과 그보다 하나 적은 기준이 필요하지만 면 { $sides }개, 기준 { $bases }개입니다
err-mixed-arity = { $line }번째 줄: { $expected }면 충돌 뒤에 { $found }면 충돌이 있습니다
err-shape = 병합에는 둘 이상의 면과 그보다 하나 적은 기준이 필요하지만 면 { $sides }개, 기준 { $bases }개입니다
err-journal-header = 저널에 온전한 헤더 줄이 없습니다
err-journal-line = 저널의 { $index }번째 줄을 해석할 수 없습니다: { $detail }
err-journal-version = 저널 형식이 { $expected }이(가) 아니라 { $found }입니다
err-journal-inputs = 저널은 다른 입력용으로 기록되었습니다
err-journal-alignment = 저널은 이 입력의 다른 정렬용으로 기록되었습니다(예: 다른 버전의 triad)
err-journal-command = 저널의 명령 { $index }을(를) 적용할 수 없습니다: { $detail }
err-cannot-open-window = 창을 열 수 없습니다: { $detail }
err-io-not-found = 그런 파일이나 디렉터리가 없습니다
err-io-permission = 권한이 거부되었습니다
err-io-exists = 파일이 이미 있습니다
err-io-is-directory = 디렉터리입니다
err-io-not-directory = 디렉터리가 아닙니다
err-io-full = 장치에 남은 공간이 없습니다
err-io-read-only = 파일 시스템이 읽기 전용입니다

## gpui-component's text

component-replace = 바꾸기
component-replace-all = 모두 바꾸기
component-clear = 지우기
