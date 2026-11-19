# A sample shell script for the highlighting tests.
set -euo pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export TARGET="${TARGET:-$ROOT/target}"
declare -a FILES=()

# Print a message to standard error.
log() {
    local level=$1
    shift
    printf '%s: %s\n' "$level" "$*" >&2
}

function count_lines {
    local path="$1"
    if [[ -f "$path" ]]; then
        wc -l < "$path"
    elif [ -d "$path" ]; then
        find "$path" -type f | wc -l
    else
        return 1
    fi
}

for file in "$ROOT"/*.toml; do
    FILES+=("$file")
done

while read -r line; do
    case "$line" in
        \#*) continue ;;
        *=*) echo "${line%%=*}" ;;
        *) log warn "skipped: $line" ;;
    esac
done < <(grep -v '^$' "$ROOT/.env" 2>/dev/null || true)

until [ "${#FILES[@]}" -eq 0 ]; do
    unset 'FILES[0]'
    FILES=("${FILES[@]}")
done

cat <<EOF > "$TARGET/summary.txt"
root: $ROOT
lines: $(count_lines "$ROOT/Cargo.toml")
EOF

total=$(( 3 * 4 + 1 ))
[ "$total" -gt 10 ] && log info "total $total" || exit 2
