# Build script integrating TLA+ model checking with Rust tests
# PowerShell version for Windows compatibility

param(
    [switch]$SkipTla,
    [switch]$SkipRust,
    [int]$Timeout = 600,
    [string]$JavaOpts = "-Xmx4g",
    [switch]$Help
)

# Colors for output (Windows PowerShell compatible)
$Red = "Red"
$Green = "Green"
$Yellow = "Yellow"
$Blue = "Blue"

if ($Help) {
    Write-Host "Usage: .\scripts\build-verify.ps1 [OPTIONS]" -ForegroundColor $Blue
    Write-Host ""
    Write-Host "Options:" -ForegroundColor $Blue
    Write-Host "  -SkipTla          Skip TLA+ model checking"
    Write-Host "  -SkipRust         Skip Rust property tests"
    Write-Host "  -Timeout SECONDS  Verification timeout (default: 600)"
    Write-Host "  -JavaOpts OPTS    Java options for TLA+ (default: -Xmx4g)"
    Write-Host "  -Help             Show this help message"
    Write-Host ""
    Write-Host "Environment Variables:" -ForegroundColor $Blue
    Write-Host "  `$env:VERIFICATION_TIMEOUT  Verification timeout in seconds"
    Write-Host "  `$env:JAVA_OPTS            Java options for TLA+ model checking"
    Write-Host "  `$env:SKIP_TLA             Skip TLA+ model checking (true/false)"
    Write-Host "  `$env:SKIP_RUST            Skip Rust property tests (true/false)"
    exit 0
}

# Override with environment variables if set
if ($env:VERIFICATION_TIMEOUT) { $Timeout = [int]$env:VERIFICATION_TIMEOUT }
if ($env:JAVA_OPTS) { $JavaOpts = $env:JAVA_OPTS }
if ($env:SKIP_TLA -eq "true") { $SkipTla = $true }
if ($env:SKIP_RUST -eq "true") { $SkipRust = $true }

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "Nyx Protocol Build & Verification Pipeline" -ForegroundColor $Blue
Write-Host "==========================================" -ForegroundColor $Blue

function Print-Status($message) {
    Write-Host "[INFO] $message" -ForegroundColor $Blue
}

function Print-Success($message) {
    Write-Host "[SUCCESS] $message" -ForegroundColor $Green
}

function Print-Warning($message) {
    Write-Host "[WARNING] $message" -ForegroundColor $Yellow
}

function Print-Error($message) {
    Write-Host "[ERROR] $message" -ForegroundColor $Red
}

# Check prerequisites
function Check-Prerequisites {
    Print-Status "Checking prerequisites..."
    
    # Check for Java
    try {
        $null = Get-Command java -ErrorAction Stop
    } catch {
        Print-Error "Java is required for TLA+ model checking"
        exit 1
    }
    
    # Check for Python
    try {
        $null = Get-Command python -ErrorAction Stop
    } catch {
        try {
            $null = Get-Command python3 -ErrorAction Stop
        } catch {
            Print-Error "Python 3 is required for verification pipeline"
            exit 1
        }
    }
    
    # Check for Cargo
    try {
        $null = Get-Command cargo -ErrorAction Stop
    } catch {
        Print-Error "Cargo is required for Rust builds"
        exit 1
    }
    
    Print-Success "Prerequisites check passed"
}

# Build Rust workspace
function Build-Rust {
    Print-Status "Building Rust workspace..."
    
    Set-Location $ProjectRoot
    
    # Build all crates
    $result = & cargo build --workspace --all-features
    if ($LASTEXITCODE -ne 0) {
        Print-Error "Rust build failed"
        exit 1
    }
    
    Print-Success "Rust build completed"
}

# Run basic Rust tests
function Test-Rust {
    Print-Status "Running Rust tests..."
    
    Set-Location $ProjectRoot
    
    # Run workspace tests
    $result = & cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) {
        Print-Error "Rust tests failed"
        exit 1
    }
    
    Print-Success "Rust tests passed"
}

# Run verification pipeline
function Run-Verification {
    Print-Status "Running verification pipeline..."
    
    Set-Location $ProjectRoot
    
    # Prepare verification arguments
    $VerifyArgs = @("--timeout", $Timeout, "--java-opts", $JavaOpts)
    
    if ($SkipTla) {
        $VerifyArgs += "--rust-only"
        Print-Warning "Skipping TLA+ model checking"
    } elseif ($SkipRust) {
        $VerifyArgs += "--tla-only"
        Print-Warning "Skipping Rust property tests"
    }
    
    $VerifyArgs += @("--output", "build_verification_report.json")
    
    # Try python3 first, then python
    $PythonCmd = "python3"
    try {
        $null = Get-Command python3 -ErrorAction Stop
    } catch {
        $PythonCmd = "python"
    }
    
    # Run verification pipeline
    $result = & $PythonCmd scripts/verify.py @VerifyArgs
    if ($LASTEXITCODE -ne 0) {
        Print-Error "Verification pipeline failed"
        
        # Show brief summary of failures if report exists
        if (Test-Path "build_verification_report.json") {
            Print-Status "Verification failure summary:"
            & $PythonCmd -c @"
import json
try:
    with open('build_verification_report.json', 'r') as f:
        report = json.load(f)
    failed = [r for r in report['results'] if not r['success']]
    for result in failed:
        print(f'  ❌ {result["name"]}: {result.get("error_message", "Unknown error")}')
except Exception as e:
    print(f'Could not parse verification report: {e}')
"@
        }
        
        exit 1
    }
    
    Print-Success "Verification pipeline completed"
}

# Generate build report
function Generate-BuildReport {
    Print-Status "Generating build report..."
    
    Set-Location $ProjectRoot
    
    # Try python3 first, then python
    $PythonCmd = "python3"
    try {
        $null = Get-Command python3 -ErrorAction Stop
    } catch {
        $PythonCmd = "python"
    }
    
    # Create build report combining verification results
    & $PythonCmd -c @"
import json
import os
import subprocess
from datetime import datetime

def run_command(cmd):
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
        return result.stdout.strip()
    except:
        return 'Unknown'

build_report = {
    'timestamp': datetime.now().isoformat(),
    'build_info': {
        'rust_version': run_command('rustc --version'),
        'cargo_version': run_command('cargo --version'),
        'java_version': run_command('java -version'),
        'python_version': run_command('python --version'),
        'platform': run_command('systeminfo | findstr /B /C:"OS Name"') if os.name == 'nt' else run_command('uname -a')
    },
    'build_status': 'success',
    'verification_included': True
}

# Include verification results if available
if os.path.exists('build_verification_report.json'):
    try:
        with open('build_verification_report.json', 'r') as f:
            verification_report = json.load(f)
        build_report['verification_results'] = verification_report
    except Exception as e:
        build_report['verification_error'] = str(e)

with open('build_report.json', 'w') as f:
    json.dump(build_report, f, indent=2)

print('Build report saved to build_report.json')
"@
    
    Print-Success "Build report generated"
}

# Main execution
function Main {
    Check-Prerequisites
    
    # Build phase
    Build-Rust
    Test-Rust
    
    # Verification phase (if not disabled)
    if (-not $SkipTla -or -not $SkipRust) {
        Run-Verification
    } else {
        Print-Warning "Verification pipeline skipped"
    }
    
    # Reporting phase
    Generate-BuildReport
    
    Print-Success "Build and verification pipeline completed successfully!"
    
    # Show summary
    if (Test-Path "build_verification_report.json") {
        Print-Status "Verification Summary:"
        
        # Try python3 first, then python
        $PythonCmd = "python3"
        try {
            $null = Get-Command python3 -ErrorAction Stop
        } catch {
            $PythonCmd = "python"
        }
        
        & $PythonCmd -c @"
import json
try:
    with open('build_verification_report.json', 'r') as f:
        report = json.load(f)
    summary = report.get('summary', {})
    print(f'  Total Verifications: {summary.get("total_verifications", 0)}')
    print(f'  Successful: {summary.get("successful_verifications", 0)}')
    print(f'  Success Rate: {summary.get("success_rate", 0):.1f}%')
    print(f'  Duration: {summary.get("total_duration_seconds", 0):.2f}s')
    
    coverage = report.get('coverage_metrics', {}).get('requirements_coverage', {})
    if coverage:
        print(f'  Requirements Coverage: {coverage.get("overall_coverage_percentage", 0):.1f}%')
except Exception as e:
    print(f'Could not parse verification summary: {e}')
"@
    }
}

# Run main function
Main