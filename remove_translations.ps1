# PowerShell script to remove non-English translations from YAML locale file

$inputFile = "locales\app.yml"
$outputFile = "locales\app_en_only.yml"

if (-not (Test-Path $inputFile)) {
    Write-Error "Input file $inputFile not found!"
    exit 1
}

Write-Host "Processing $inputFile..."

# Read all lines from the input file
$lines = Get-Content $inputFile -Encoding UTF8

$outputLines = @()
$skipUntilNextSection = $false
$currentIndent = 0

for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]

    # Skip empty lines - always include them
    if ($line -match '^\s*$') {
        $outputLines += $line
        continue
    }

    # Calculate current line's indentation
    $matches = [regex]::Match($line, '^(\s*)')
    $lineIndent = $matches.Groups[1].Value.Length

    # Check if this is a language key line
    if ($line -match '^\s*(zh-CN|zh-TW|ru|vi|ja|es|pt|ko):\s*"') {
        # Skip this non-English translation
        continue
    }

    # Check if this is an English key line
    if ($line -match '^\s*en:\s*"') {
        # Include English translation
        $outputLines += $line
        continue
    }

    # For any other line (section headers, etc.)
    $outputLines += $line
}

# Write the output file
$outputLines | Out-File -FilePath $outputFile -Encoding UTF8

Write-Host "English-only locale file created: $outputFile"

# Show file size comparison
$originalSize = (Get-Item $inputFile).Length
$newSize = (Get-Item $outputFile).Length

Write-Host "Original file size: $originalSize bytes"
Write-Host "New file size: $newSize bytes"
Write-Host "Reduction: $([math]::Round((($originalSize - $newSize) / $originalSize * 100), 2))%"
