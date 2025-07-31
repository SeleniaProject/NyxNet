# Nyx Protocol Verification Scripts

This directory contains the automated verification pipeline for the Nyx protocol, integrating TLA+ model checking with Rust property-based tests.

## Scripts Overview

### Core Verification Scripts

#### `verify.py`
Main verification pipeline that orchestrates TLA+ model checking and Rust property-based tests.

**Usage:**
```bash
python3 scripts/verify.py [options]
```

**Options:**
- `--timeout SECONDS`: Verification timeout (default: 600)
- `--java-opts OPTS`: Java options for TLA+ (default: "-Xmx4g")
- `--tla-only`: Run only TLA+ model checking
- `--rust-only`: Run only Rust property tests
- `--output FILE`: Output file for report (default: "verification_report.json")

**Example:**
```bash
# Full verification pipeline
python3 scripts/verify.py --timeout 900 --output full_report.json

# TLA+ only with more memory
python3 scripts/verify.py --tla-only --java-opts "-Xmx8g"

# Rust property tests only
python3 scripts/verify.py --rust-only
```

#### `generate-verification-report.py`
Generates comprehensive verification reports with coverage metrics and requirement traceability.

**Usage:**
```bash
python3 scripts/generate-verification-report.py verification_report.json [options]
```

**Options:**
- `--output FILE`: Output JSON report (default: "verification_coverage_report.json")
- `--html FILE`: Generate HTML report
- `--project-root DIR`: Project root directory (default: ".")

**Example:**
```bash
# Generate comprehensive report with HTML output
python3 scripts/generate-verification-report.py verification_report.json \
    --output coverage_report.json \
    --html verification_report.html
```

### Build Integration Scripts

#### `build-verify.sh` (Linux/macOS)
Build script that integrates verification with the Rust build process.

**Usage:**
```bash
./scripts/build-verify.sh [options]
```

**Options:**
- `--skip-tla`: Skip TLA+ model checking
- `--skip-rust`: Skip Rust property tests
- `--timeout SECONDS`: Verification timeout
- `--java-opts OPTS`: Java options for TLA+

**Environment Variables:**
- `VERIFICATION_TIMEOUT`: Verification timeout in seconds
- `JAVA_OPTS`: Java options for TLA+ model checking
- `SKIP_TLA`: Skip TLA+ model checking (true/false)
- `SKIP_RUST`: Skip Rust property tests (true/false)

#### `build-verify.ps1` (Windows)
PowerShell version of the build script for Windows compatibility.

**Usage:**
```powershell
.\scripts\build-verify.ps1 [options]
```

**Options:**
- `-SkipTla`: Skip TLA+ model checking
- `-SkipRust`: Skip Rust property tests
- `-Timeout SECONDS`: Verification timeout
- `-JavaOpts OPTS`: Java options for TLA+

### Cargo Integration

#### `cargo-verify`
Custom Cargo command for running formal verification.

**Installation:**
```bash
# Make executable
chmod +x scripts/cargo-verify

# Add to PATH or create symlink
ln -s $(pwd)/scripts/cargo-verify ~/.cargo/bin/cargo-verify
```

**Usage:**
```bash
cargo verify [options]
```

**Options:**
- `--tla-only`: Run only TLA+ model checking
- `--rust-only`: Run only Rust property tests
- `--timeout SECONDS`: Verification timeout
- `--quick`: Run quick verification (basic TLA+ only)
- `--html-report FILE`: Generate HTML report

**Examples:**
```bash
# Full verification
cargo verify

# Quick verification for development
cargo verify --quick

# TLA+ only with HTML report
cargo verify --tla-only --html-report tla_report.html

# Rust property tests with extended timeout
cargo verify --rust-only --timeout 1200
```

## Verification Pipeline Architecture

### 1. TLA+ Model Checking
- **Model**: `formal/nyx_multipath_plugin.tla`
- **Configurations**: Multiple TLC configurations for different scenarios
- **Properties**: Safety invariants and liveness properties
- **Output**: State exploration statistics and counterexamples

### 2. Rust Property-Based Testing
- **Location**: `nyx-conformance/tests/`
- **Framework**: Proptest for property-based testing
- **Coverage**: Protocol state machine, multipath selection, capability negotiation, cryptographic operations, network simulation
- **Output**: Test results and property violation examples

### 3. Coverage Analysis
- **Requirements Traceability**: Maps verification results to formal requirements
- **Code Coverage**: Rust code coverage analysis (with cargo-tarpaulin if available)
- **TLA+ Coverage**: Model checking configuration and property coverage
- **Integration Coverage**: Cross-verification between TLA+ and Rust tests

### 4. Reporting System
- **JSON Reports**: Machine-readable verification results
- **HTML Reports**: Human-readable coverage reports with visualizations
- **CI Integration**: GitHub Actions workflow integration
- **Metrics**: Composite scoring system for verification quality

## CI/CD Integration

### GitHub Actions Workflows

#### `formal-verification.yml`
Comprehensive formal verification workflow that runs on:
- Push to main/develop branches
- Pull requests
- Weekly schedule
- Manual dispatch

**Features:**
- Multi-platform testing (Ubuntu, Windows)
- Parallel TLA+ and Rust verification
- Comprehensive reporting
- Artifact collection
- Status checks

#### `tla-ci.yml`
Lightweight TLA+ model checking for quick feedback:
- Runs on formal/ directory changes
- Basic model checking with caching
- Fast feedback for TLA+ model changes

### Integration with Existing CI

The verification pipeline integrates with existing CI workflows:

```yaml
# Add to existing CI workflow
- name: Run formal verification
  run: |
    python3 scripts/verify.py --timeout 600
  continue-on-error: false

- name: Upload verification results
  uses: actions/upload-artifact@v4
  with:
    name: verification-results
    path: verification_report.json
```

## Development Workflow

### 1. Local Development
```bash
# Quick verification during development
cargo verify --quick

# Full verification before commit
cargo verify --html-report verification.html
```

### 2. Pre-commit Hooks
```bash
# Add to .git/hooks/pre-commit
#!/bin/bash
echo "Running formal verification..."
if ! python3 scripts/verify.py --timeout 300; then
    echo "Formal verification failed. Commit aborted."
    exit 1
fi
```

### 3. Release Process
```bash
# Comprehensive verification for releases
./scripts/build-verify.sh --timeout 1800
python3 scripts/generate-verification-report.py build_verification_report.json \
    --html release_verification_report.html
```

## Configuration

### Environment Variables
- `VERIFICATION_TIMEOUT`: Default timeout for verification steps
- `JAVA_OPTS`: Java options for TLA+ model checking
- `SKIP_TLA`: Skip TLA+ model checking in build scripts
- `SKIP_RUST`: Skip Rust property tests in build scripts
- `PROPTEST_CASES`: Number of property test cases to run
- `PROPTEST_RNG_SEED`: Seed for property test randomization

### TLA+ Configuration
TLA+ model checking configurations are in `formal/`:
- `basic.cfg`: Quick smoke test (30s)
- `comprehensive.cfg`: Full verification (5-10min)
- `scalability.cfg`: Large-scale testing (15-30min)
- `capability_stress.cfg`: Capability negotiation stress test (10-20min)
- `liveness_focus.cfg`: Temporal properties focus (2-5min)

### Rust Test Configuration
Property-based tests in `nyx-conformance/tests/`:
- `multipath_selection_properties.rs`: Path generation and validation
- `capability_negotiation_properties.rs`: Handshake protocol verification
- `protocol_state_machine_properties.rs`: State transition testing
- `cryptographic_operation_properties.rs`: Cryptographic property verification
- `network_simulation_properties.rs`: End-to-end protocol behavior

## Troubleshooting

### Common Issues

#### TLA+ Model Checking Fails
```bash
# Check Java version and memory
java -version
java -Xmx4g -version

# Run with verbose output
cd formal
java -Xmx4g -cp tla2tools.jar tlc2.TLC -config basic.cfg nyx_multipath_plugin.tla
```

#### Rust Property Tests Fail
```bash
# Run specific test with verbose output
cd nyx-conformance
cargo test multipath_selection_properties --verbose

# Run with specific seed for reproducibility
PROPTEST_RNG_SEED=42 cargo test
```

#### Verification Pipeline Hangs
```bash
# Check for deadlocks or infinite loops
ps aux | grep -E "(java|cargo|python)"

# Kill hanging processes
pkill -f "tlc2.TLC"
pkill -f "cargo test"
```

#### Memory Issues
```bash
# Increase Java heap size
export JAVA_OPTS="-Xmx8g"

# Monitor memory usage
htop
```

### Performance Optimization

#### TLA+ Model Checking
- Increase Java heap size for larger models
- Use symmetry reduction in TLC configurations
- Parallelize model checking with multiple workers
- Cache TLA+ tools jar file

#### Rust Property Testing
- Adjust `PROPTEST_CASES` for test thoroughness vs. speed
- Use release builds for property tests
- Parallelize test execution with `cargo test --jobs N`

#### CI/CD Optimization
- Cache dependencies and build artifacts
- Use matrix builds for parallel execution
- Optimize artifact collection and storage
- Use conditional workflows for relevant changes

## Metrics and Reporting

### Verification Metrics
- **Composite Score**: Weighted average of all verification aspects
- **Verification Success Rate**: Percentage of successful verifications
- **Requirements Coverage**: Percentage of requirements covered by verification
- **TLA+ Coverage**: Percentage of model properties verified
- **Code Coverage**: Percentage of Rust code covered by tests

### Grading System
- **A+ (95-100%)**: Excellent verification coverage
- **A (90-94%)**: Very good verification coverage
- **A- (85-89%)**: Good verification coverage
- **B+ (80-84%)**: Acceptable verification coverage
- **B (75-79%)**: Needs improvement
- **C+ (65-74%)**: Significant gaps in verification
- **C (60-64%)**: Major verification issues
- **F (<60%)**: Inadequate verification coverage

### Report Formats
- **JSON**: Machine-readable for CI/CD integration
- **HTML**: Human-readable with visualizations
- **Console**: Quick feedback during development
- **GitHub Actions Summary**: Integrated CI/CD reporting

## Contributing

### Adding New Verification Tests

#### TLA+ Model Extensions
1. Extend `formal/nyx_multipath_plugin.tla` with new properties
2. Add corresponding TLC configuration file
3. Update `scripts/verify.py` to include new configuration
4. Add requirement mapping in report generator

#### Rust Property Tests
1. Create new test file in `nyx-conformance/tests/`
2. Implement property-based tests using proptest
3. Update `scripts/verify.py` to run new test category
4. Add requirement traceability mapping

#### Verification Pipeline Enhancements
1. Extend `VerificationPipeline` class in `scripts/verify.py`
2. Add new metrics to coverage analyzer
3. Update report generator with new visualizations
4. Add CI/CD workflow integration

### Code Style and Standards
- Follow Python PEP 8 for Python scripts
- Use type hints for better code documentation
- Add comprehensive docstrings for all functions
- Include error handling and logging
- Write unit tests for verification pipeline components

## License

This verification infrastructure is part of the Nyx protocol project and follows the same licensing terms.