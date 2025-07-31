# Nyx Protocol Formal Verification

This directory contains the formal verification infrastructure for the Nyx protocol, including TLA+ models, TLC model checking configurations, and automated verification tools.

## Files Overview

### TLA+ Model
- `nyx_multipath_plugin.tla` - Main TLA+ specification with formal proofs
- Contains safety invariants, liveness properties, and TLAPS proofs

### TLC Configuration Files

#### Basic Configuration (`basic.cfg`)
- **Purpose**: Quick smoke test with minimal parameters
- **Node Count**: 8 nodes
- **Capabilities**: 3 capability types
- **Properties**: Basic invariants and termination
- **Runtime**: ~30 seconds

#### Comprehensive Configuration (`comprehensive.cfg`)
- **Purpose**: Full verification with moderate scale
- **Node Count**: 15 nodes  
- **Capabilities**: 8 capability types
- **Properties**: All invariants plus liveness properties
- **Runtime**: ~5-10 minutes

#### Scalability Configuration (`scalability.cfg`)
- **Purpose**: Test model behavior at larger scale
- **Node Count**: 25 nodes
- **Capabilities**: 10 capability types
- **Properties**: Core invariants and termination
- **Runtime**: ~15-30 minutes

#### Capability Stress Configuration (`capability_stress.cfg`)
- **Purpose**: Stress test capability negotiation logic
- **Node Count**: 12 nodes
- **Capabilities**: 15 capability types
- **Properties**: All properties including progress guarantees
- **Runtime**: ~10-20 minutes

#### Liveness Focus Configuration (`liveness_focus.cfg`)
- **Purpose**: Focus on temporal properties and fairness
- **Specification**: Uses FairSpec with fairness constraints
- **Properties**: Emphasizes liveness and progress properties
- **Runtime**: ~2-5 minutes

### Automation Tools

#### Model Checking Script (`run_model_checking.py`)
Automated Python script that:
- Runs all TLC configurations sequentially
- Collects performance metrics and statistics
- Analyzes counterexamples when violations are found
- Generates comprehensive JSON reports
- Provides colored terminal output with progress indicators

## Prerequisites

1. **TLA+ Tools**: Download `tla2tools.jar` from [TLA+ releases](https://github.com/tlaplus/tlaplus/releases)
2. **Java**: Java 8 or higher
3. **Python**: Python 3.7+ (for automation script)

## Usage

### Manual TLC Execution

Run individual configurations:

```bash
# Basic smoke test
java -Xmx2g -cp tla2tools.jar tlc2.TLC -config basic.cfg nyx_multipath_plugin.tla

# Comprehensive verification
java -Xmx4g -cp tla2tools.jar tlc2.TLC -config comprehensive.cfg nyx_multipath_plugin.tla

# Scalability test
java -Xmx8g -cp tla2tools.jar tlc2.TLC -config scalability.cfg nyx_multipath_plugin.tla
```

### Automated Execution

Run all configurations with the automation script:

```bash
# Run all configurations with default settings
python run_model_checking.py

# Custom timeout and memory settings
python run_model_checking.py --timeout 600 --java-opts "-Xmx8g"

# Custom output file
python run_model_checking.py --output verification_results.json
```

### Script Options

- `--timeout`: Timeout per configuration in seconds (default: 300)
- `--java-opts`: Java options for TLC (default: "-Xmx4g")
- `--output`: Output file for JSON report (default: "model_checking_report.json")
- `--config-dir`: Directory containing config files (default: ".")

## Output and Reports

### Terminal Output
The script provides real-time feedback:
- ✓ Configuration passed with timing and state count
- ✗ Configuration failed with error details
- ⚠ Configuration timed out
- 🔍 Counterexample analysis when violations found

### JSON Report
Comprehensive report includes:
- Summary statistics (success rate, total duration, states explored)
- Per-configuration results with detailed metrics
- Counterexamples with structured analysis
- Timestamps for reproducibility

### Counterexample Analysis
When violations are found, the script automatically:
- Extracts the counterexample trace
- Identifies the type of violation (safety vs liveness)
- Analyzes the state sequence leading to the violation
- Provides human-readable explanation of the issue

## Verification Properties

### Safety Invariants
- **TypeInvariant**: All variables maintain correct types
- **Inv_PathLen**: Path length stays within 3-7 nodes
- **Inv_NoDup**: No duplicate nodes in paths
- **Inv_Error**: Error states are consistent
- **Inv_NoError**: Success states have no errors
- **Inv_PowerState**: Power state transitions are valid
- **CapabilityInvariant**: Capability negotiation consistency

### Liveness Properties
- **Terminating**: System eventually leaves initial state
- **ProgressToOpen**: Compatible capabilities lead to open state
- **ProgressToClose**: Incompatible capabilities lead to close state

### Formal Proofs
The TLA+ model includes TLAPS proofs for:
- Main safety theorem proving all invariants
- Main liveness theorem proving progress properties
- Individual theorems for each safety property
- Transition validity proofs

## Performance Expectations

| Configuration | Expected Runtime | Memory Usage | States Explored |
|---------------|------------------|--------------|-----------------|
| basic         | 30s             | 2GB          | ~10,000         |
| comprehensive | 5-10min         | 4GB          | ~100,000        |
| scalability   | 15-30min        | 8GB          | ~500,000        |
| capability_stress | 10-20min     | 4GB          | ~200,000        |
| liveness_focus | 2-5min         | 2GB          | ~50,000         |

## Troubleshooting

### Common Issues

1. **OutOfMemoryError**: Increase Java heap size with `-Xmx` option
2. **Timeout**: Increase timeout value or reduce model parameters
3. **TLA+ tools not found**: Ensure `tla2tools.jar` is in the current directory
4. **Counterexample found**: Review the analysis output and check model logic

### Debugging Tips

1. Start with `basic.cfg` to verify setup
2. Use smaller parameter values for debugging
3. Check TLC output for specific error messages
4. Review counterexample traces step by step
5. Verify TLA+ syntax with TLA+ IDE

## Integration with CI/CD

The automation script returns appropriate exit codes:
- Exit 0: All configurations passed
- Exit 1: One or more configurations failed

Example CI integration:
```yaml
- name: Run formal verification
  run: |
    cd formal
    python run_model_checking.py --timeout 600
```

## Contributing

When modifying the TLA+ model:
1. Run `basic.cfg` first to catch syntax errors
2. Verify all configurations still pass
3. Update this README if adding new configurations
4. Include TLAPS proofs for new invariants
5. Test with the automation script before committing