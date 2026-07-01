param(
    [int]$MaxLines = 499
)

$ErrorActionPreference = 'Stop'
$violations = 0

Get-ChildItem -Path src, tests -Filter '*.rs' -Recurse -File -ErrorAction SilentlyContinue |
    Sort-Object FullName |
    ForEach-Object {
        $lineCount = (Get-Content -LiteralPath $_.FullName | Measure-Object -Line).Lines
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
