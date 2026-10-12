# Runs every check that has to pass before a commit, one after another, and
# prints one line for each. From the repo root, in Windows PowerShell:
#
#   .\tools\check-all.ps1          # everything (about four minutes)
#   .\tools\check-all.ps1 -Quick   # leaves out the end-to-end tests
#   .\tools\check-all.ps1 -Sim     # then also 300 whole runs in the sim
#
# The full output of a step that fails is in tools\.check-logs\<step>.log.

param(
    [switch]$Quick,
    [switch]$Sim
)

$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root 'web'
$logs = Join-Path $PSScriptRoot '.check-logs'
New-Item -ItemType Directory -Force $logs | Out-Null
$failed = @()

function Step($name, $folder, $command, $summary) {
    $log = Join-Path $logs "$name.log"
    Push-Location $folder
    $started = Get-Date
    # Through cmd, so that a tool writing to stderr is not taken for a failure.
    cmd /c "$command > `"$log`" 2>&1"
    $ok = $LASTEXITCODE -eq 0
    Pop-Location
    $seconds = [int]((Get-Date) - $started).TotalSeconds
    $line = ''
    if ($summary) {
        $found = Select-String -Path $log -Pattern $summary | Select-Object -Last 1
        if ($found) { $line = $found.Line.Trim() }
    }
    $mark = if ($ok) { 'ok  ' } else { 'FAIL' }
    Write-Host ("{0} {1,-10} {2,4}s  {3}" -f $mark, $name, $seconds, $line)
    if (-not $ok) { $script:failed += $name }
}

Step 'fmt' $root 'cargo fmt --check' $null
Step 'clippy' $root 'cargo clippy --all-targets -- -D warnings' $null
Step 'cargo-test' $root 'cargo test' '^test result: \w+\. [1-9]'
Step 'scryfall' $root 'node tools/scryfall.mjs check' 'cards checked'
Step 'tools' $root 'node --test tools/scryfall.test.mjs' 'pass \d+'
Step 'wasm' $web 'npm run wasm' 'Done in'
Step 'web-test' $web 'npm test' '^\s*Tests\s'
Step 'lint' $web 'npm run lint' $null
Step 'build' $web 'npm run build' 'built in'
if (-not $Quick) {
    Step 'e2e' $web 'npm run e2e' '\d+ passed'
}
if ($Sim) {
    Step 'sim-runs' $root 'cargo run --release -p sim -- --runs --games 300' 'The whole run took'
    Select-String -Path (Join-Path $logs 'sim-runs.log') -Pattern 'runs$|Won the act|errors' |
        ForEach-Object { Write-Host "     $($_.Line.Trim())" }
}

if ($failed.Count -gt 0) {
    Write-Host "Failed: $($failed -join ', '). See tools\.check-logs."
    exit 1
}
Write-Host 'All checks passed.'
