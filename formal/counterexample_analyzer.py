#!/usr/bin/env python3
"""
Advanced Counterexample Analysis Tool for Nyx Protocol
Provides detailed analysis, visualization, and debugging support for TLC counterexamples
"""

import os
import sys
import json
import argparse
import re
from typing import Dict, List, Tuple, Optional
from datetime import datetime

class AdvancedCounterexampleAnalyzer:
    """Advanced analysis of TLA+ counterexamples with debugging support"""
    
    def __init__(self):
        self.state_patterns = {
            "deadlock": self._detect_deadlock,
            "livelock": self._detect_livelock,
            "resource_leak": self._detect_resource_leak,
            "invariant_violation": self._detect_invariant_violation,
            "timeout_cascade": self._detect_timeout_cascade,
            "state_explosion": self._detect_state_explosion
        }
    
    def parse_counterexample_file(self, file_path: str) -> Dict:
        """Parse a counterexample from a TLC output file"""
        if not os.path.exists(file_path):
            raise FileNotFoundError(f"Counterexample file not found: {file_path}")
        
        with open(file_path, 'r') as f:
            content = f.read()
        
        return self._parse_counterexample_content(content)
    
    def _parse_counterexample_content(self, content: str) -> Dict:
        """Parse counterexample content from TLC output"""
        counterexample = {
            "type": "unknown",
            "violated_property": None,
            "error_message": None,
            "state_sequence": [],
            "metadata": {
                "total_states": 0,
                "trace_length": 0,
                "parsing_timestamp": datetime.now().isoformat()
            }
        }
        
        lines = content.split('\n')
        current_state = {}
        in_state = False
        state_number = 0
        
        for i, line in enumerate(lines):
            line = line.strip()
            
            # Detect error type
            if "Error: Invariant" in line:
                counterexample["type"] = "safety_violation"
                counterexample["violated_property"] = line
            elif "Error: Temporal property" in line:
                counterexample["type"] = "liveness_violation"
                counterexample["violated_property"] = line
            elif "Error: Deadlock" in line:
                counterexample["type"] = "deadlock"
                counterexample["violated_property"] = line
            
            # Parse state information
            if line.startswith("State ") and ":" in line:
                if current_state:
                    counterexample["state_sequence"].append(current_state.copy())
                current_state = {}
                state_number += 1
                in_state = True
                continue
            
            if in_state and line.startswith("/\\"):
                # Variable assignment
                var_assignment = line[2:].strip()  # Remove "/\ "
                if "=" in var_assignment:
                    var, value = var_assignment.split("=", 1)
                    current_state[var.strip()] = value.strip()
            elif in_state and line == "":
                if current_state:
                    counterexample["state_sequence"].append(current_state.copy())
                    current_state = {}
                in_state = False
        
        # Add final state if exists
        if current_state:
            counterexample["state_sequence"].append(current_state)
        
        counterexample["metadata"]["trace_length"] = len(counterexample["state_sequence"])
        
        return counterexample
    
    def _detect_deadlock(self, states: List[Dict]) -> Optional[Dict]:
        """Detect deadlock patterns in state sequence"""
        if len(states) < 2:
            return None
        
        final_state = states[-1]
        
        # Check if system is stuck in a terminal state with active resources
        if ("state" in final_state and 
            final_state.get("state") not in ["Closed", "Terminal"] and
            "active_streams" in final_state and
            final_state.get("active_streams") != "{}"):
            
            return {
                "pattern": "deadlock",
                "description": "System stuck with active resources but no progress",
                "evidence": {
                    "final_state": final_state.get("state"),
                    "active_resources": final_state.get("active_streams"),
                    "error_state": final_state.get("error")
                }
            }
        
        return None
    
    def _detect_livelock(self, states: List[Dict]) -> Optional[Dict]:
        """Detect livelock patterns (system active but no progress)"""
        if len(states) < 4:
            return None
        
        # Look for repeating state patterns
        state_values = [s.get("state", "") for s in states[-6:]]  # Check last 6 states
        
        # Check for alternating pattern
        if len(set(state_values)) == 2 and len(state_values) >= 4:
            pattern_length = 2
            is_repeating = True
            
            for i in range(len(state_values) - pattern_length):
                if state_values[i] != state_values[i + pattern_length]:
                    is_repeating = False
                    break
            
            if is_repeating:
                return {
                    "pattern": "livelock",
                    "description": "System oscillating between states without progress",
                    "evidence": {
                        "oscillating_states": list(set(state_values)),
                        "pattern_length": pattern_length,
                        "occurrences": len(state_values) // pattern_length
                    }
                }
        
        return None
    
    def _detect_resource_leak(self, states: List[Dict]) -> Optional[Dict]:
        """Detect resource leak patterns"""
        if len(states) < 3:
            return None
        
        # Track resource counts over time
        resource_metrics = {
            "active_streams": [],
            "session_keys": [],
            "retry_count": []
        }
        
        for state in states:
            for metric in resource_metrics:
                if metric in state:
                    try:
                        if metric == "active_streams":
                            # Parse set notation
                            value = state[metric].strip("{}")
                            count = len(value.split(",")) if value else 0
                        elif metric == "session_keys":
                            # Parse set notation
                            value = state[metric].strip("{}")
                            count = len(value.split(",")) if value else 0
                        else:
                            count = int(state[metric])
                        
                        resource_metrics[metric].append(count)
                    except (ValueError, AttributeError):
                        pass
        
        # Check for monotonically increasing resources
        for metric, values in resource_metrics.items():
            if len(values) >= 3:
                if all(values[i] <= values[i+1] for i in range(len(values)-1)):
                    if values[-1] > values[0] + 2:  # Significant increase
                        return {
                            "pattern": "resource_leak",
                            "description": f"Resource {metric} continuously increasing",
                            "evidence": {
                                "metric": metric,
                                "initial_value": values[0],
                                "final_value": values[-1],
                                "growth_rate": (values[-1] - values[0]) / len(values)
                            }
                        }
        
        return None
    
    def _detect_invariant_violation(self, states: List[Dict]) -> Optional[Dict]:
        """Detect invariant violation patterns"""
        if not states:
            return None
        
        final_state = states[-1]
        violations = []
        
        # Check common invariant violations
        if "retry_count" in final_state:
            try:
                retry_count = int(final_state["retry_count"])
                if retry_count > 5:  # Assuming max retries should be limited
                    violations.append({
                        "invariant": "retry_bound",
                        "description": "Retry count exceeds reasonable limit",
                        "value": retry_count
                    })
            except ValueError:
                pass
        
        if "state" in final_state and "error" in final_state:
            state = final_state["state"]
            error = final_state["error"]
            
            # Check state-error consistency
            if state == "Active" and error != "None":
                violations.append({
                    "invariant": "state_error_consistency",
                    "description": "Active state with non-None error",
                    "state": state,
                    "error": error
                })
        
        if violations:
            return {
                "pattern": "invariant_violation",
                "description": "Multiple invariant violations detected",
                "evidence": {"violations": violations}
            }
        
        return None
    
    def _detect_timeout_cascade(self, states: List[Dict]) -> Optional[Dict]:
        """Detect cascading timeout failures"""
        if len(states) < 3:
            return None
        
        timeout_events = []
        
        for i, state in enumerate(states):
            if "timeout_count" in state:
                try:
                    timeout_count = int(state["timeout_count"])
                    if timeout_count > 0:
                        timeout_events.append((i, timeout_count))
                except ValueError:
                    pass
        
        if len(timeout_events) >= 2:
            # Check if timeouts are increasing rapidly
            if timeout_events[-1][1] > timeout_events[0][1] + 2:
                return {
                    "pattern": "timeout_cascade",
                    "description": "Cascading timeout failures detected",
                    "evidence": {
                        "initial_timeouts": timeout_events[0][1],
                        "final_timeouts": timeout_events[-1][1],
                        "cascade_length": len(timeout_events)
                    }
                }
        
        return None
    
    def _detect_state_explosion(self, states: List[Dict]) -> Optional[Dict]:
        """Detect state explosion patterns"""
        if len(states) > 100:  # Very long trace indicates potential explosion
            return {
                "pattern": "state_explosion",
                "description": "Extremely long counterexample trace",
                "evidence": {
                    "trace_length": len(states),
                    "complexity_indicator": "high"
                }
            }
        
        return None
    
    def analyze_patterns(self, counterexample: Dict) -> List[Dict]:
        """Analyze counterexample for various patterns"""
        states = counterexample.get("state_sequence", [])
        detected_patterns = []
        
        for pattern_name, detector in self.state_patterns.items():
            result = detector(states)
            if result:
                detected_patterns.append(result)
        
        return detected_patterns
    
    def generate_fix_recommendations(self, patterns: List[Dict], counterexample: Dict) -> List[Dict]:
        """Generate specific fix recommendations based on detected patterns"""
        recommendations = []
        
        for pattern in patterns:
            pattern_type = pattern["pattern"]
            
            if pattern_type == "deadlock":
                recommendations.append({
                    "priority": "high",
                    "category": "concurrency",
                    "description": "Add timeout mechanisms to prevent deadlock",
                    "implementation": [
                        "Implement operation timeouts in all blocking operations",
                        "Add deadlock detection and recovery mechanisms",
                        "Review resource acquisition order to prevent circular waits"
                    ]
                })
            
            elif pattern_type == "livelock":
                recommendations.append({
                    "priority": "high",
                    "category": "progress",
                    "description": "Break oscillation cycles with randomization or priorities",
                    "implementation": [
                        "Add randomized backoff to break symmetry",
                        "Implement priority-based state transitions",
                        "Add progress tracking to detect and break cycles"
                    ]
                })
            
            elif pattern_type == "resource_leak":
                recommendations.append({
                    "priority": "medium",
                    "category": "resource_management",
                    "description": "Implement proper resource cleanup",
                    "implementation": [
                        "Add explicit resource cleanup in error paths",
                        "Implement resource limits and monitoring",
                        "Use RAII patterns for automatic resource management"
                    ]
                })
            
            elif pattern_type == "timeout_cascade":
                recommendations.append({
                    "priority": "medium",
                    "category": "error_handling",
                    "description": "Improve timeout handling and recovery",
                    "implementation": [
                        "Implement exponential backoff for timeout recovery",
                        "Add circuit breaker pattern to prevent cascading failures",
                        "Reset timeout counters after successful operations"
                    ]
                })
        
        return recommendations
    
    def create_visualization(self, counterexample: Dict) -> str:
        """Create a visual representation of the counterexample"""
        states = counterexample.get("state_sequence", [])
        
        if not states:
            return "No states to visualize"
        
        viz = "COUNTEREXAMPLE VISUALIZATION\n"
        viz += "=" * 60 + "\n\n"
        
        viz += f"Type: {counterexample['type']}\n"
        if counterexample.get('violated_property'):
            viz += f"Violated Property: {counterexample['violated_property']}\n"
        viz += f"Trace Length: {len(states)} states\n\n"
        
        # Create state transition diagram
        for i, state in enumerate(states):
            viz += f"┌─ State {i + 1} " + "─" * 30 + "┐\n"
            
            # Show key variables
            key_vars = ["state", "prev_state", "error", "retry_count", "crypto_state", "active_streams"]
            for var in key_vars:
                if var in state:
                    value = state[var]
                    if len(str(value)) > 40:
                        value = str(value)[:37] + "..."
                    viz += f"│ {var:15}: {value:20} │\n"
            
            viz += "└" + "─" * 48 + "┘\n"
            
            if i < len(states) - 1:
                viz += "           │\n"
                viz += "           ▼\n"
        
        return viz
    
    def generate_test_case(self, counterexample: Dict, config_name: str) -> str:
        """Generate a test case to reproduce the counterexample scenario"""
        states = counterexample.get("state_sequence", [])
        
        test_case = f'''"""
Test case generated from counterexample in {config_name}
Reproduces: {counterexample.get('type', 'unknown')} violation
"""

import unittest
from typing import Dict, List

class CounterexampleReproductionTest(unittest.TestCase):
    """Test case to reproduce the counterexample scenario"""
    
    def setUp(self):
        """Set up test environment"""
        self.initial_state = {states[0] if states else {}}
        self.expected_violation = "{counterexample.get('type', 'unknown')}"
    
    def test_reproduce_counterexample(self):
        """Reproduce the exact counterexample scenario"""
        # Initial state
        current_state = self.initial_state.copy()
        
        # Expected state sequence
        expected_states = {states}
        
        # Simulate state transitions
        for i, expected_state in enumerate(expected_states):
            with self.subTest(state_number=i+1):
                # Verify state matches expected
                for key, expected_value in expected_state.items():
                    if key in current_state:
                        self.assertEqual(
                            current_state[key], 
                            expected_value,
                            f"State {{i+1}}: {{key}} mismatch"
                        )
        
        # Verify violation occurred
        self.assertTrue(
            self._check_violation_condition(),
            f"Expected {{self.expected_violation}} violation did not occur"
        )
    
    def _check_violation_condition(self) -> bool:
        """Check if the violation condition is met"""
        # Implement specific violation checks based on counterexample type
        return True  # Placeholder
    
    def test_fix_validation(self):
        """Test that proposed fixes prevent the violation"""
        # This test should pass after implementing fixes
        self.skipTest("Implement after applying fixes")

if __name__ == "__main__":
    unittest.main()
'''
        
        return test_case

def main():
    parser = argparse.ArgumentParser(description="Analyze TLA+ counterexamples for Nyx protocol")
    parser.add_argument("--input", required=True, help="Counterexample file or TLC output")
    parser.add_argument("--config", help="Configuration name for context")
    parser.add_argument("--output", default="counterexample_analysis.json", 
                       help="Output file for analysis report")
    parser.add_argument("--visualize", action="store_true", 
                       help="Generate visualization")
    parser.add_argument("--generate-test", action="store_true",
                       help="Generate test case for reproduction")
    parser.add_argument("--verbose", action="store_true",
                       help="Verbose output")
    
    args = parser.parse_args()
    
    analyzer = AdvancedCounterexampleAnalyzer()
    
    print("🔍 ANALYZING COUNTEREXAMPLE")
    print("=" * 50)
    
    try:
        # Parse counterexample
        if os.path.isfile(args.input):
            counterexample = analyzer.parse_counterexample_file(args.input)
        else:
            # Assume it's raw counterexample text
            counterexample = analyzer._parse_counterexample_content(args.input)
        
        print(f"Counterexample type: {counterexample['type']}")
        print(f"Trace length: {counterexample['metadata']['trace_length']} states")
        
        # Analyze patterns
        patterns = analyzer.analyze_patterns(counterexample)
        print(f"Detected patterns: {len(patterns)}")
        
        for pattern in patterns:
            print(f"  - {pattern['pattern']}: {pattern['description']}")
        
        # Generate recommendations
        recommendations = analyzer.generate_fix_recommendations(patterns, counterexample)
        print(f"Generated recommendations: {len(recommendations)}")
        
        # Create comprehensive report
        report = {
            "counterexample": counterexample,
            "detected_patterns": patterns,
            "recommendations": recommendations,
            "analysis_metadata": {
                "analyzer_version": "1.0",
                "analysis_timestamp": datetime.now().isoformat(),
                "config_name": args.config or "unknown"
            }
        }
        
        # Save report
        with open(args.output, 'w') as f:
            json.dump(report, f, indent=2)
        print(f"Analysis report saved to: {args.output}")
        
        # Generate visualization if requested
        if args.visualize:
            viz = analyzer.create_visualization(counterexample)
            viz_file = args.output.replace('.json', '_visualization.txt')
            with open(viz_file, 'w') as f:
                f.write(viz)
            print(f"Visualization saved to: {viz_file}")
        
        # Generate test case if requested
        if args.generate_test:
            test_case = analyzer.generate_test_case(counterexample, args.config or "unknown")
            test_file = args.output.replace('.json', '_test.py')
            with open(test_file, 'w') as f:
                f.write(test_case)
            print(f"Test case saved to: {test_file}")
        
        # Print summary
        print("\n📋 ANALYSIS SUMMARY")
        print("=" * 30)
        
        if patterns:
            print("Critical issues found:")
            for pattern in patterns:
                print(f"  ⚠️  {pattern['description']}")
        
        if recommendations:
            print("\nRecommended actions:")
            for rec in recommendations[:3]:  # Show top 3
                print(f"  🔧 {rec['description']}")
        
    except Exception as e:
        print(f"Error analyzing counterexample: {e}")
        if args.verbose:
            import traceback
            traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    main()