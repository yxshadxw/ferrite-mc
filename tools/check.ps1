#Requires -Version 5.1
<#
.SYNOPSIS
    One-shot local CI for Ferrite, mirroring .github/workflows/ci.yml.

.DESCRIPTION
    Runs, in order and stopping at the first failure:
        cargo fmt --all --check
        cargo clippy --workspace --all-targets --all-features -- -D warnings
        cargo test --workspace --all-features
    then prints a PASS/FAIL line with the total elapsed seconds.

.PARAMETER Quick
    Skip cargo clippy.

.EXAMPLE
    .\tools\check.ps1

.EXAMPLE
    .\tools\check.ps1 -Quick
#>
[CmdletBinding()]
param(
    [switch]$Quick
)

$ErrorActionPreference = 'Continue'

# Rust lives in %USERPROFILE%\.cargo\bin and is not on PATH by default
# (see tools/env.ps1).
$env:Path = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:Path

$repoRoot = Split-Path -Parent $PSScriptRoot
$originalLocation = Get-Location
$checkWatch = [System.Diagnostics.Stopwatch]::StartNew()

function Write-Fail {
    param([string]$Message)

    $totalSeconds = [int]($checkWatch.Elapsed.TotalSeconds)
    Write-Host ''
    Write-Host ("FAIL: {0} ({1}s)" -f $Message, $totalSeconds) -ForegroundColor Red
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Fail 'cargo not found on PATH or in ~\.cargo\bin'
    exit 1
}

$steps = @(
    @{ Label = 'cargo fmt --all --check'; Args = @('fmt', '--all', '--check') },
    @{ Label = 'cargo clippy --workspace --all-targets --all-features -- -D warnings'; Args = @('clippy', '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings') },
    @{ Label = 'cargo test --workspace --all-features'; Args = @('test', '--workspace', '--all-features') }
)

$failed = $null
$skipped = 0

Set-Location -LiteralPath $repoRoot
try {
    foreach ($step in $steps) {
        if ($Quick -and $step.Label.StartsWith('cargo clippy')) {
            Write-Host ''
            Write-Host '==> skipping cargo clippy (-Quick)' -ForegroundColor DarkGray
            $skipped++
            continue
        }

        $stepWatch = [System.Diagnostics.Stopwatch]::StartNew()
        Write-Host ''
        Write-Host ('==> ' + $step.Label) -ForegroundColor Cyan

        $cargoArgs = $step.Args
        & cargo @cargoArgs
        $code = $LASTEXITCODE
        $seconds = [int]($stepWatch.Elapsed.TotalSeconds)

        if ($code -ne 0) {
            Write-Host ("--- {0} failed in {1}s (exit {2})" -f $step.Label, $seconds, $code) -ForegroundColor Red
            $failed = $step.Label
            break
        }

        Write-Host ("--- {0} passed in {1}s" -f $step.Label, $seconds) -ForegroundColor Green
    }
}
finally {
    Set-Location -LiteralPath $originalLocation.Path
}

if ($null -ne $failed) {
    Write-Fail $failed
    exit 1
}

$totalSeconds = [int]($checkWatch.Elapsed.TotalSeconds)
if ($skipped -gt 0) {
    Write-Host ''
    Write-Host ("PASS: all checks passed ({0}s, quick: clippy skipped)" -f $totalSeconds) -ForegroundColor Green
}
else {
    Write-Host ''
    Write-Host ("PASS: all checks passed ({0}s)" -f $totalSeconds) -ForegroundColor Green
}

exit 0
