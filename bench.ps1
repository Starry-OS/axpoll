# Runs the axpoll micro-benchmark in release mode.
#
# Usage:
#   .\bench.ps1            # run once
#   .\bench.ps1 before     # run with a label (useful for before/after comparison)
param(
    [string]$Label = ""
)

$ErrorActionPreference = "Stop"

if ($Label -ne "") {
    Write-Host "===== bench: $Label ====="
}

cargo run --release --example bench
