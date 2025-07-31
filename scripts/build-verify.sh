#!/bin/bash
# Build script integrating TLA+ model checking with Rust tests
# This script is designed to be run as part of the build process

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERIFICATION_TIMEOUT=${VERIFICATION_TIMEOUT:-600}
JAVA_OPTS=${JAVA_OPTS:-"-Xmx4g"}
SKIP_TLA=${SKIP_TLA:-false}
SKIP_RUST=${SKIP_RUST:-false}

echo -e "${BLUE}Nyx Protocol Build & Verification Pipeline${NC}"
echo "=========================================="

# Function to print status messages
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    print_status "Checking prerequisites..."
    
    # Check for Java
    if ! command -v java &> /dev/null; then
        print_error "Java is required for TLA+ model checking"
        exit 1
    fi
    
    # Check for Python
    if ! command -v python3 &> /dev/null; then
        print_error "Python 3 is required for verification pipeline"
        exit 1
    fi
    
    # Check for Cargo
    if ! command -v cargo &> /dev/null; then
        print_error "Cargo is required for Rust builds"
        exit 1
    fi
    
    print_success "Prerequisites check passed"
}

# Build Rust workspace
build_rust() {
    print_status "Building Rust workspace..."
    
    cd "$PROJECT_ROOT"
    
    # Build all crates
    if ! cargo build --workspace --all-features; then
        print_error "Rust build failed"
        exit 1
    fi
    
    print_success "Rust build completed"
}

# Run basic Rust tests
test_rust() {
    print_status "Running Rust tests..."
    
    cd "$PROJECT_ROOT"
    
    # Run workspace tests
    if ! cargo test --workspace --all-features; then
        print_error "Rust tests failed"
        exit 1
    fi
    
    print_success "Rust tests passed"
}

# Run verification pipeline
run_verification() {
    print_status "Running verification pipeline..."
    
    cd "$PROJECT_ROOT"
    
    # Prepare verification arguments
    VERIFY_ARGS="--timeout $VERIFICATION_TIMEOUT --java-opts '$JAVA_OPTS'"
    
    if [ "$SKIP_TLA" = "true" ]; then
        VERIFY_ARGS="$VERIFY_ARGS --rust-only"
        print_warning "Skipping TLA+ model checking"
    elif [ "$SKIP_RUST" = "true" ]; then
        VERIFY_ARGS="$VERIFY_ARGS --tla-only"
        print_warning "Skipping Rust property tests"
    fi
    
    # Run verification pipeline
    if ! python3 scripts/verify.py $VERIFY_ARGS --output "build_verification_report.json"; then
        print_error "Verification pipeline failed"
        
        # Show brief summary of failures if report exists
        if [ -f "build_verification_report.json" ]; then
            print_status "Verification failure summary:"
            python3 -c "
import json
try:
    with open('build_verification_report.json', 'r') as f:
        report = json.load(f)
    failed = [r for r in report['results'] if not r['success']]
    for result in failed:
        print(f'  ❌ {result[\"name\"]}: {result.get(\"error_message\", \"Unknown error\")}')
except Exception as e:
    print(f'Could not parse verification report: {e}')
"
        fi
        
        exit 1
    fi
    
    print_success "Verification pipeline completed"
}

# Generate build report
generate_build_report() {
    print_status "Generating build report..."
    
    cd "$PROJECT_ROOT"
    
    # Create build report combining verification results
    python3 -c "
import json
import os
from datetime import datetime

build_report = {
    'timestamp': datetime.now().isoformat(),
    'build_info': {
        'rust_version': os.popen('rustc --version').read().strip(),
        'cargo_version': os.popen('cargo --version').read().strip(),
        'java_version': os.popen('java -version 2>&1 | head -1').read().strip(),
        'python_version': os.popen('python3 --version').read().strip(),
        'platform': os.popen('uname -a').read().strip()
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
"
    
    print_success "Build report generated"
}

# Main execution
main() {
    check_prerequisites
    
    # Build phase
    build_rust
    test_rust
    
    # Verification phase (if not disabled)
    if [ "$SKIP_TLA" != "true" ] || [ "$SKIP_RUST" != "true" ]; then
        run_verification
    else
        print_warning "Verification pipeline skipped"
    fi
    
    # Reporting phase
    generate_build_report
    
    print_success "Build and verification pipeline completed successfully!"
    
    # Show summary
    if [ -f "build_verification_report.json" ]; then
        print_status "Verification Summary:"
        python3 -c "
import json
try:
    with open('build_verification_report.json', 'r') as f:
        report = json.load(f)
    summary = report.get('summary', {})
    print(f'  Total Verifications: {summary.get(\"total_verifications\", 0)}')
    print(f'  Successful: {summary.get(\"successful_verifications\", 0)}')
    print(f'  Success Rate: {summary.get(\"success_rate\", 0):.1f}%')
    print(f'  Duration: {summary.get(\"total_duration_seconds\", 0):.2f}s')
    
    coverage = report.get('coverage_metrics', {}).get('requirements_coverage', {})
    if coverage:
        print(f'  Requirements Coverage: {coverage.get(\"overall_coverage_percentage\", 0):.1f}%')
except Exception as e:
    print(f'Could not parse verification summary: {e}')
"
    fi
}

# Handle command line arguments
while [[ \$# -gt 0 ]]; do
    case \$1 in
        --skip-tla)
            SKIP_TLA=true
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        --timeout)
            VERIFICATION_TIMEOUT="\$2"
            shift 2
            ;;
        --java-opts)
            JAVA_OPTS="\$2"
            shift 2
            ;;
        --help)
            echo "Usage: \$0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-tla          Skip TLA+ model checking"
            echo "  --skip-rust         Skip Rust property tests"
            echo "  --timeout SECONDS   Verification timeout (default: 600)"
            echo "  --java-opts OPTS    Java options for TLA+ (default: -Xmx4g)"
            echo "  --help              Show this help message"
            echo ""
            echo "Environment Variables:"
            echo "  VERIFICATION_TIMEOUT  Verification timeout in seconds"
            echo "  JAVA_OPTS            Java options for TLA+ model checking"
            echo "  SKIP_TLA             Skip TLA+ model checking (true/false)"
            echo "  SKIP_RUST            Skip Rust property tests (true/false)"
            exit 0
            ;;
        *)
            print_error "Unknown option: \$1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Run main function
main