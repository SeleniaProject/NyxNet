# Script to remove all C/C++ dependencies from NyxNet project

Write-Host "Removing all C/C++ dependencies from NyxNet project..." -ForegroundColor Yellow

# Remove pqcrypto dependencies from all Cargo.toml files
Get-ChildItem -Path . -Name "Cargo.toml" -Recurse | ForEach-Object {
    $file = $_
    Write-Host "Processing $file..." -ForegroundColor Cyan
    
    $content = Get-Content $file -Raw
    
    # Remove pqcrypto-* dependencies
    $content = $content -replace 'pqcrypto-[a-zA-Z0-9-]+ = [^\r\n]+\r?\n?', ''
    
    # Remove ring dependencies
    $content = $content -replace 'ring = [^\r\n]+\r?\n?', ''
    
    # Remove openssl dependencies
    $content = $content -replace 'openssl[a-zA-Z0-9-]* = [^\r\n]+\r?\n?', ''
    
    # Remove cc build dependencies
    $content = $content -replace 'cc = [^\r\n]+\r?\n?', ''
    
    # Remove cmake dependencies
    $content = $content -replace 'cmake = [^\r\n]+\r?\n?', ''
    
    # Remove pkg-config dependencies
    $content = $content -replace 'pkg-config = [^\r\n]+\r?\n?', ''
    
    Set-Content -Path $file -Value $content -NoNewline
}

Write-Host "C/C++ dependencies removal completed!" -ForegroundColor Green
