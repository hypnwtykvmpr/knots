param(
    [int]$MaxLines = 499
)

$ErrorActionPreference = 'Stop'
$violations = 0

Get-ChildItem -Path src, tests -Filter '*.rs' -Recurse -File -ErrorAction SilentlyContinue |
    Sort-Object FullName |
    ForEach-Object {
        # Physical line count (blank lines included) to match the Linux
        # gate's `wc -l`; Measure-Object -Line skips blank lines and let
        # oversized files pass silently on Windows.
        $lineCount = @(Get-Content -LiteralPath $_.FullName).Count
        if ($lineCount -gt $MaxLines) {
            $relative = Resolve-Path -Relative -LiteralPath $_.FullName
            Write-Output "error: $relative is $lineCount lines (max $MaxLines)"
            $script:violations++
        }
    }

if ($violations -gt 0) {
    Write-Output "$violations file-size violation(s) found."
    exit 1
}

Write-Output "All Rust files are within the $MaxLines-line limit."
