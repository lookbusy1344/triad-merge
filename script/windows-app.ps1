<#
.SYNOPSIS
Builds triad-merge.exe and triad-merge-gui.exe on Windows with the build line the About window
shows: the commit, and the jj change in a jj repository.

.DESCRIPTION
Names the revision as script/build-id does and passes it to the compiler in TRIAD_COMMIT and
TRIAD_CHANGE. jj names the head of the non-empty revisions in ::@, where a merge counts as
non-empty. git names HEAD, with -dirty when the working tree has changes; a clean HEAD that jj
wrote names its change too, from the commit's change-id header. Outside a repository the build
has no build line. The session's environment is left as it was.

.EXAMPLE
script/windows-app.ps1
target\release\triad-merge*.exe, the fast local build.

.EXAMPLE
script/windows-app.ps1 dist
target\dist\triad-merge*.exe, with fat LTO, as the release workflow builds them.

.EXAMPLE
script/windows-app.ps1 -BuildId
Prints the exports script/build-id prints for the current directory, and builds nothing.
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('release', 'dist')]
    [string] $CargoProfile = 'release',

    [switch] $BuildId
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$IdLength = 12
$JjTip = 'heads(::@ ~ (empty() ~ merges()))'
$Package = 'triad-merge'
$Binaries = @('triad-merge.exe', 'triad-merge-gui.exe')
$IdVariables = @('TRIAD_COMMIT', 'TRIAD_CHANGE')

# The revision the build comes from, as an ordered map of TRIAD_COMMIT and TRIAD_CHANGE; empty
# outside a repository.
function Get-BuildIds {
    $ids = [ordered]@{}
    jj --ignore-working-copy root *> $null
    if ($LASTEXITCODE -eq 0) {
        $ids.TRIAD_COMMIT = jj log --no-graph -r $JjTip -T "commit_id.short($IdLength)"
        if ($LASTEXITCODE -ne 0) { throw "jj log failed with exit code $LASTEXITCODE" }
        $ids.TRIAD_CHANGE = jj log --no-graph -r $JjTip -T "change_id.short($IdLength)"
        if ($LASTEXITCODE -ne 0) { throw "jj log failed with exit code $LASTEXITCODE" }
        return $ids
    }
    $commit = git rev-parse --short=$IdLength HEAD 2> $null
    if ($LASTEXITCODE -ne 0) { return $ids }
    if (git status --porcelain) {
        $ids.TRIAD_COMMIT = "$commit-dirty"
        return $ids
    }
    $ids.TRIAD_COMMIT = $commit
    # The headers end at the first blank line; the message follows.
    $headers = git cat-file commit HEAD | ForEach-Object -Begin { $done = $false } -Process {
        if ($_ -eq '') { $done = $true }
        if (-not $done) { $_ }
    }
    $change = $headers | Where-Object { $_ -like 'change-id *' } | Select-Object -First 1
    if ($change) {
        $ids.TRIAD_CHANGE = $change.Substring('change-id '.Length, $IdLength)
    }
    $ids
}

if ($BuildId) {
    $ids = Get-BuildIds
    foreach ($name in $ids.Keys) { "export $name=$($ids[$name])" }
    return
}

if (-not $IsWindows) {
    Write-Error 'script/windows-app.ps1: Windows only'
    exit 2
}

$saved = @{}
foreach ($name in $IdVariables) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $ids = Get-BuildIds
    foreach ($name in $IdVariables) {
        [Environment]::SetEnvironmentVariable($name, $ids[$name])
    }
    cargo build --profile $CargoProfile --locked -p $Package
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $out = Join-Path (Resolve-Path ($env:CARGO_TARGET_DIR ?? 'target')) $CargoProfile
    if ($ids.Count -eq 0) {
        Write-Host 'Build line: none (not in a repository)'
    } else {
        Write-Host "Build line: $(($ids.Values) -join ', ')"
    }
    foreach ($binary in $Binaries) { Write-Host (Join-Path $out $binary) }
} finally {
    Pop-Location
    foreach ($name in $IdVariables) {
        [Environment]::SetEnvironmentVariable($name, $saved[$name])
    }
}
