# platform.sh — sourced by the gate scripts, so they run the same under Git Bash on Windows
# as on macOS and Linux.
#
#   on_windows  true under Git Bash on Windows.
#   jq          jq; on Windows it writes LF line ends, as elsewhere.
#   python3     a Python 3 interpreter, LF line ends on stdout.
#   native_pwd  the current directory as native tools print it: C:/dev/triad on Windows.
#   slash_paths read paths from stdin, write them with `/` separators.
#
# Compare a path that cargo prints only after `gsub("\\\\"; "/")` in jq or `slash_paths`.

if [[ ${OSTYPE} == msys || ${OSTYPE} == cygwin ]]; then
    on_windows() { true; }
    # Native jq.exe and python.exe write CRLF to a pipe unless told otherwise.
    jq() { command jq --binary "$@"; }
    python3() {
        local python
        # type -P searches PATH alone; command -v would find this function.
        python=$(type -P python3 || type -P python) || {
            echo "error: Python 3 not found on PATH" >&2
            return 127
        }
        # MSYS rewrites arguments that look like POSIX paths; pass them as written.
        MSYS2_ARG_CONV_EXCL='*' PYTHONIOENCODING=utf-8 "${python}" "$@" | tr -d '\r'
        return "${PIPESTATUS[0]}"
    }
    native_pwd() { pwd -W; }
    slash_paths() { tr '\\' /; }
else
    on_windows() { false; }
    native_pwd() { pwd -P; }
    slash_paths() { cat; }
fi
