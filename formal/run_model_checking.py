#!/usr/bin/env python3
"""
Automated TLC Model Checking Script for Nyx Protocol
Runs comprehensive model checking with various parameter combinations
and provides counterexample analysis and reporting functionality.
"""

import os
import sys
import subprocess
import json
import time
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import argparse

class ModelCheckingResult:
    def __init__(self, config_name: str, success: bool, duration: float, 
                 states_generated: int, distinct_states: int, 
                 error_message: Optional[str] = None, 
                 counterexample: Optional[str] = None):
        self.config_name = config_name
        self.success = success
        self.duration = duration
        self.states_generated = states_generated
        self.distinct_states = distinct_states
        self.error_message = error_message
        self.counterexample = counterexample
        self.timestamp = datetime.now().isoformat()

class TLCRunner:
    def __init__(self, tla_file: str = "nyx_multipath_plugin.tla", 
                 java_opts: str = "-Xmx4g"):
        self.tla_file = tla_file
        self.java_opts = java_opts
        self.results: List[ModelCheckingResult] = []
        self.state_space_stats = {}
        self.coverage_data = {}
        
    def run_tlc(self, config_file: str, timeout: int = 300, 
                optimization_flags: List[str] = None) -> ModelCheckingResult:
        """Run TLC model checker with specified configuration"""
        config_name = Path(config_file).stem
        print(f"Running TLC with configuration: {config_name}")
        
        cmd = [
            "java", self.java_opts, "-cp", "tla2tools.jar",
            "tlc2.TLC", "-config", config_file
        ]
        
        # Add optimization flags for state space exploration
        if optimization_flags:
            cmd.extend(optimization_flags)
        else:
            # Default optimizations for efficient exploration
            cmd.extend([
                "-workers", "auto",  # Use all available CPU cores
                "-dfid", "10",       # Depth-first iterative deepening
                "-checkpoint", "10", # Checkpoint every 10 minutes
                "-coverage", "10"    # Coverage statistics every 10 minutes
            ])
        
        cmd.append(self.tla_file)
        
        start_time = time.time()
        try:
            result = subprocess.run(
                cmd, 
                capture_output=True, 
                text=True, 
                timeout=timeout,
                cwd=os.path.dirname(os.path.abspath(__file__))
            )
            duration = time.time() - start_time
            
            # Parse TLC output for statistics
            states_generated, distinct_states = self._parse_tlc_stats(result.stdout)
            
            # Analyze state space exploration
            state_space_analysis = self._analyze_state_space(config_name, result.stdout)
            
            # Extract coverage information
            coverage_info = self._extract_coverage_info(result.stdout)
            
            if result.returncode == 0:
                print(f"✓ {config_name}: SUCCESS ({duration:.2f}s, {states_generated} states)")
                
                # Store coverage data
                self.coverage_data[config_name] = coverage_info
                
                return ModelCheckingResult(
                    config_name, True, duration, states_generated, distinct_states
                )
            else:
                # Check for counterexample
                counterexample = self._extract_counterexample(result.stdout)
                error_msg = self._extract_error_message(result.stdout, result.stderr)
                
                print(f"✗ {config_name}: FAILED ({duration:.2f}s)")
                if counterexample:
                    print(f"  Counterexample found")
                if error_msg:
                    print(f"  Error: {error_msg}")
                
                return ModelCheckingResult(
                    config_name, False, duration, states_generated, distinct_states,
                    error_msg, counterexample
                )
                
        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            print(f"⚠ {config_name}: TIMEOUT ({timeout}s)")
            return ModelCheckingResult(
                config_name, False, duration, 0, 0, "Timeout expired"
            )
        except Exception as e:
            duration = time.time() - start_time
            print(f"✗ {config_name}: ERROR - {str(e)}")
            return ModelCheckingResult(
                config_name, False, duration, 0, 0, str(e)
            )
    
    def _parse_tlc_stats(self, output: str) -> Tuple[int, int]:
        """Extract state statistics from TLC output"""
        states_generated = 0
        distinct_states = 0
        
        for line in output.split('\n'):
            if "states generated" in line:
                try:
                    states_generated = int(line.split()[0])
                except (ValueError, IndexError):
                    pass
            elif "distinct states found" in line:
                try:
                    distinct_states = int(line.split()[0])
                except (ValueError, IndexError):
                    pass
        
        return states_generated, distinct_states
    
    def _analyze_state_space(self, config_name: str, output: str) -> Dict:
        """Analyze state space exploration efficiency"""
        analysis = {
            "state_space_efficiency": 0.0,
            "exploration_depth": 0,
            "branching_factor": 0.0,
            "coverage_metrics": {},
            "optimization_suggestions": []
        }
        
        lines = output.split('\n')
        states_generated = 0
        distinct_states = 0
        max_depth = 0
        
        for line in lines:
            if "states generated" in line:
                try:
                    states_generated = int(line.split()[0])
                except (ValueError, IndexError):
                    pass
            elif "distinct states found" in line:
                try:
                    distinct_states = int(line.split()[0])
                except (ValueError, IndexError):
                    pass
            elif "depth" in line.lower() and "reached" in line.lower():
                try:
                    # Extract depth information
                    depth_match = [int(s) for s in line.split() if s.isdigit()]
                    if depth_match:
                        max_depth = max(depth_match)
                except (ValueError, IndexError):
                    pass
        
        # Calculate efficiency metrics
        if states_generated > 0:
            analysis["state_space_efficiency"] = distinct_states / states_generated
        
        if distinct_states > 0 and max_depth > 0:
            analysis["branching_factor"] = distinct_states ** (1.0 / max_depth)
        
        analysis["exploration_depth"] = max_depth
        
        # Generate optimization suggestions
        if analysis["state_space_efficiency"] < 0.5:
            analysis["optimization_suggestions"].append(
                "Consider using symmetry reduction to improve state space efficiency"
            )
        
        if max_depth > 50:
            analysis["optimization_suggestions"].append(
                "Deep state space detected - consider bounded model checking"
            )
        
        if states_generated > 1000000:
            analysis["optimization_suggestions"].append(
                "Large state space - consider using state space reduction techniques"
            )
        
        self.state_space_stats[config_name] = analysis
        return analysis
    
    def _extract_coverage_info(self, output: str) -> Dict:
        """Extract coverage information from TLC output"""
        coverage = {
            "action_coverage": {},
            "state_coverage": {},
            "invariant_coverage": {},
            "property_coverage": {}
        }
        
        lines = output.split('\n')
        in_coverage_section = False
        
        for line in lines:
            line = line.strip()
            
            if "Coverage" in line or "Action" in line:
                in_coverage_section = True
                continue
            
            if in_coverage_section and line:
                # Parse coverage statistics
                if ":" in line and "%" in line:
                    parts = line.split(":")
                    if len(parts) >= 2:
                        action_name = parts[0].strip()
                        coverage_text = parts[1].strip()
                        
                        # Extract percentage
                        if "%" in coverage_text:
                            try:
                                percentage = float(coverage_text.split("%")[0].split()[-1])
                                coverage["action_coverage"][action_name] = percentage
                            except (ValueError, IndexError):
                                pass
        
        return coverage
    
    def generate_coverage_report(self, config_name: str) -> Dict:
        """Generate comprehensive coverage report for a configuration"""
        if config_name not in self.coverage_data:
            return {"error": "No coverage data available"}
        
        coverage_data = self.coverage_data[config_name]
        state_space_data = self.state_space_stats.get(config_name, {})
        
        report = {
            "configuration": config_name,
            "state_space_analysis": state_space_data,
            "coverage_summary": {
                "total_actions": len(coverage_data.get("action_coverage", {})),
                "covered_actions": sum(1 for cov in coverage_data.get("action_coverage", {}).values() if cov > 0),
                "average_coverage": 0.0
            },
            "detailed_coverage": coverage_data,
            "recommendations": []
        }
        
        # Calculate average coverage
        action_coverages = list(coverage_data.get("action_coverage", {}).values())
        if action_coverages:
            report["coverage_summary"]["average_coverage"] = sum(action_coverages) / len(action_coverages)
        
        # Generate recommendations
        low_coverage_actions = [
            action for action, cov in coverage_data.get("action_coverage", {}).items() 
            if cov < 50.0
        ]
        
        if low_coverage_actions:
            report["recommendations"].append(
                f"Low coverage detected for actions: {', '.join(low_coverage_actions[:5])}"
            )
        
        if report["coverage_summary"]["average_coverage"] < 70.0:
            report["recommendations"].append(
                "Overall coverage is low - consider increasing model bounds or adding test scenarios"
            )
        
        return report
    
    def _extract_counterexample(self, output: str) -> Optional[str]:
        """Extract counterexample from TLC output if present"""
        lines = output.split('\n')
        counterexample_lines = []
        in_counterexample = False
        
        for line in lines:
            if "Error: Invariant" in line or "Error: Temporal property" in line:
                in_counterexample = True
                counterexample_lines.append(line)
            elif in_counterexample:
                if line.strip() == "" and len(counterexample_lines) > 10:
                    break
                counterexample_lines.append(line)
        
        return '\n'.join(counterexample_lines) if counterexample_lines else None
    
    def _extract_error_message(self, stdout: str, stderr: str) -> Optional[str]:
        """Extract error message from TLC output"""
        for line in stdout.split('\n'):
            if line.startswith("Error:"):
                return line
        
        if stderr.strip():
            return stderr.strip()
        
        return None
    
    def run_all_configurations(self, config_dir: str = ".", timeout: int = 300) -> List[ModelCheckingResult]:
        """Run model checking on all configuration files"""
        config_files = [
            "minimal_test.cfg",
            "basic.cfg",
            "comprehensive.cfg",
            "enhanced_comprehensive.cfg",
            "scalability.cfg",
            "capability_stress.cfg",
            "stress_testing.cfg",
            "liveness_focus.cfg"
        ]
        
        print("Starting comprehensive TLC model checking...")
        print("=" * 60)
        
        for config_file in config_files:
            config_path = os.path.join(config_dir, config_file)
            if os.path.exists(config_path):
                result = self.run_tlc(config_path, timeout)
                self.results.append(result)
            else:
                print(f"⚠ Configuration file not found: {config_file}")
        
        return self.results
    
    def run_optimized_exploration(self, config_file: str, timeout: int = 600) -> ModelCheckingResult:
        """Run model checking with optimized state space exploration"""
        optimization_flags = [
            "-workers", "auto",
            "-dfid", "20",           # Deeper iterative deepening
            "-checkpoint", "5",      # More frequent checkpoints
            "-coverage", "5",        # More frequent coverage reports
            "-deadlock",             # Check for deadlocks
            "-simulate",             # Enable simulation mode for large state spaces
            "num=1000",              # Number of simulation runs
            "-depth", "100"          # Maximum simulation depth
        ]
        
        return self.run_tlc(config_file, timeout, optimization_flags)
    
    def analyze_all_coverage(self) -> Dict:
        """Generate comprehensive coverage analysis for all configurations"""
        analysis = {
            "timestamp": datetime.now().isoformat(),
            "configurations": {},
            "summary": {
                "total_configs": len(self.coverage_data),
                "average_coverage": 0.0,
                "best_coverage_config": None,
                "worst_coverage_config": None
            }
        }
        
        coverage_scores = {}
        
        for config_name in self.coverage_data:
            config_report = self.generate_coverage_report(config_name)
            analysis["configurations"][config_name] = config_report
            
            avg_coverage = config_report["coverage_summary"]["average_coverage"]
            coverage_scores[config_name] = avg_coverage
        
        if coverage_scores:
            analysis["summary"]["average_coverage"] = sum(coverage_scores.values()) / len(coverage_scores)
            analysis["summary"]["best_coverage_config"] = max(coverage_scores, key=coverage_scores.get)
            analysis["summary"]["worst_coverage_config"] = min(coverage_scores, key=coverage_scores.get)
        
        return analysis
    
    def generate_report(self, output_file: str = "model_checking_report.json") -> None:
        """Generate comprehensive report of model checking results"""
        report = {
            "timestamp": datetime.now().isoformat(),
            "summary": {
                "total_configurations": len(self.results),
                "successful": sum(1 for r in self.results if r.success),
                "failed": sum(1 for r in self.results if not r.success),
                "total_duration": sum(r.duration for r in self.results),
                "total_states": sum(r.states_generated for r in self.results)
            },
            "results": []
        }
        
        for result in self.results:
            result_data = {
                "config_name": result.config_name,
                "success": result.success,
                "duration_seconds": result.duration,
                "states_generated": result.states_generated,
                "distinct_states": result.distinct_states,
                "timestamp": result.timestamp
            }
            
            if result.error_message:
                result_data["error_message"] = result.error_message
            
            if result.counterexample:
                result_data["counterexample"] = result.counterexample
            
            report["results"].append(result_data)
        
        with open(output_file, 'w') as f:
            json.dump(report, f, indent=2)
        
        print(f"\nReport saved to: {output_file}")
    
    def print_summary(self) -> None:
        """Print summary of model checking results"""
        print("\n" + "=" * 60)
        print("MODEL CHECKING SUMMARY")
        print("=" * 60)
        
        successful = [r for r in self.results if r.success]
        failed = [r for r in self.results if not r.success]
        
        print(f"Total configurations: {len(self.results)}")
        print(f"Successful: {len(successful)}")
        print(f"Failed: {len(failed)}")
        print(f"Total duration: {sum(r.duration for r in self.results):.2f}s")
        print(f"Total states explored: {sum(r.states_generated for r in self.results):,}")
        
        if successful:
            print(f"\n✓ SUCCESSFUL CONFIGURATIONS:")
            for result in successful:
                print(f"  {result.config_name}: {result.states_generated:,} states in {result.duration:.2f}s")
        
        if failed:
            print(f"\n✗ FAILED CONFIGURATIONS:")
            for result in failed:
                print(f"  {result.config_name}: {result.error_message or 'Unknown error'}")
                if result.counterexample:
                    print(f"    Counterexample available")

class CounterexampleAnalyzer:
    """Analyze and report on counterexamples found during model checking"""
    
    @staticmethod
    def analyze_counterexample(counterexample: str) -> Dict:
        """Analyze counterexample and extract key information"""
        analysis = {
            "type": "unknown",
            "violated_property": None,
            "state_sequence": [],
            "key_variables": {},
            "analysis": ""
        }
        
        lines = counterexample.split('\n')
        
        # Determine type of violation
        for line in lines:
            if "Invariant" in line:
                analysis["type"] = "safety_violation"
                analysis["violated_property"] = line.strip()
            elif "Temporal property" in line:
                analysis["type"] = "liveness_violation"
                analysis["violated_property"] = line.strip()
        
        # Extract state information
        current_state = {}
        for line in lines:
            line = line.strip()
            if line.startswith("/\\"):
                # Variable assignment
                if "=" in line:
                    var_assignment = line[2:].strip()  # Remove "/\ "
                    if "=" in var_assignment:
                        var, value = var_assignment.split("=", 1)
                        current_state[var.strip()] = value.strip()
            elif line == "":
                if current_state:
                    analysis["state_sequence"].append(current_state.copy())
                    current_state = {}
        
        # Add final state if exists
        if current_state:
            analysis["state_sequence"].append(current_state)
        
        # Generate analysis text
        if analysis["type"] == "safety_violation":
            analysis["analysis"] = CounterexampleAnalyzer._analyze_safety_violation(analysis)
        elif analysis["type"] == "liveness_violation":
            analysis["analysis"] = CounterexampleAnalyzer._analyze_liveness_violation(analysis)
        
        return analysis
    
    @staticmethod
    def _analyze_safety_violation(analysis: Dict) -> str:
        """Generate analysis text for safety violations"""
        if not analysis["state_sequence"]:
            return "No state sequence available for analysis"
        
        final_state = analysis["state_sequence"][-1]
        
        analysis_text = "Safety violation analysis:\n"
        analysis_text += f"- Violated property: {analysis['violated_property']}\n"
        analysis_text += f"- Number of states in trace: {len(analysis['state_sequence'])}\n"
        
        if "state" in final_state:
            analysis_text += f"- Final state: {final_state['state']}\n"
        
        if "error" in final_state:
            analysis_text += f"- Error code: {final_state['error']}\n"
        
        if "path" in final_state:
            analysis_text += f"- Final path: {final_state['path']}\n"
        
        return analysis_text
    
    @staticmethod
    def _analyze_liveness_violation(analysis: Dict) -> str:
        """Generate analysis text for liveness violations"""
        analysis_text = "Liveness violation analysis:\n"
        analysis_text += f"- Violated property: {analysis['violated_property']}\n"
        analysis_text += f"- The system failed to make progress\n"
        analysis_text += f"- Check for deadlock or infinite loops in the model\n"
        
        return analysis_text
    
    @staticmethod
    def generate_counterexample_visualization(counterexample: str) -> str:
        """Generate a visual representation of the counterexample trace"""
        analysis = CounterexampleAnalyzer.analyze_counterexample(counterexample)
        
        if not analysis["state_sequence"]:
            return "No state sequence available for visualization"
        
        visualization = "COUNTEREXAMPLE TRACE VISUALIZATION\n"
        visualization += "=" * 50 + "\n\n"
        
        for i, state in enumerate(analysis["state_sequence"]):
            visualization += f"State {i + 1}:\n"
            visualization += "-" * 20 + "\n"
            
            # Extract key state variables
            key_vars = ["state", "error", "retry_count", "path", "crypto_state", "active_streams"]
            
            for var in key_vars:
                if var in state:
                    visualization += f"  {var}: {state[var]}\n"
            
            if i < len(analysis["state_sequence"]) - 1:
                visualization += "    ↓\n"
            
            visualization += "\n"
        
        return visualization
    
    @staticmethod
    def identify_bug_patterns(counterexample: str) -> List[str]:
        """Identify common bug patterns in the counterexample"""
        analysis = CounterexampleAnalyzer.analyze_counterexample(counterexample)
        patterns = []
        
        if not analysis["state_sequence"]:
            return patterns
        
        states = analysis["state_sequence"]
        
        # Check for infinite retry loops
        retry_counts = [int(s.get("retry_count", "0")) for s in states if "retry_count" in s]
        if retry_counts and max(retry_counts) >= 3:
            patterns.append("INFINITE_RETRY_LOOP: System stuck in retry loop")
        
        # Check for state oscillation
        state_values = [s.get("state", "") for s in states if "state" in s]
        if len(state_values) > 3:
            # Look for repeated state patterns
            for i in range(len(state_values) - 2):
                if i + 4 < len(state_values):
                    if (state_values[i] == state_values[i + 2] and 
                        state_values[i + 1] == state_values[i + 3]):
                        patterns.append("STATE_OSCILLATION: System oscillating between states")
                        break
        
        # Check for resource leaks
        stream_counts = []
        for s in states:
            if "active_streams" in s:
                try:
                    # Parse set notation like {1,2,3}
                    streams_str = s["active_streams"].strip("{}")
                    if streams_str:
                        count = len(streams_str.split(","))
                    else:
                        count = 0
                    stream_counts.append(count)
                except:
                    pass
        
        if stream_counts and len(stream_counts) > 1:
            if all(count >= stream_counts[0] for count in stream_counts[1:]):
                patterns.append("RESOURCE_LEAK: Active streams never decrease")
        
        # Check for timeout accumulation
        timeout_counts = [int(s.get("timeout_count", "0")) for s in states if "timeout_count" in s]
        if timeout_counts and len(timeout_counts) > 1:
            if timeout_counts[-1] > timeout_counts[0] + 2:
                patterns.append("TIMEOUT_ACCUMULATION: Excessive timeout events")
        
        # Check for capability negotiation failures
        error_values = [s.get("error", "") for s in states if "error" in s]
        if "UNSUPPORTED_CAP" in error_values:
            patterns.append("CAPABILITY_MISMATCH: Capability negotiation failed")
        
        return patterns
    
    @staticmethod
    def suggest_fixes(counterexample: str) -> List[str]:
        """Suggest potential fixes based on counterexample analysis"""
        patterns = CounterexampleAnalyzer.identify_bug_patterns(counterexample)
        analysis = CounterexampleAnalyzer.analyze_counterexample(counterexample)
        
        suggestions = []
        
        for pattern in patterns:
            if "INFINITE_RETRY_LOOP" in pattern:
                suggestions.append("Add maximum retry limit enforcement in recovery logic")
                suggestions.append("Implement exponential backoff for retry attempts")
            
            elif "STATE_OSCILLATION" in pattern:
                suggestions.append("Add state transition guards to prevent oscillation")
                suggestions.append("Implement state history tracking to detect cycles")
            
            elif "RESOURCE_LEAK" in pattern:
                suggestions.append("Ensure proper resource cleanup in error paths")
                suggestions.append("Add resource limit enforcement")
            
            elif "TIMEOUT_ACCUMULATION" in pattern:
                suggestions.append("Reset timeout counters after successful operations")
                suggestions.append("Implement timeout threshold limits")
            
            elif "CAPABILITY_MISMATCH" in pattern:
                suggestions.append("Improve capability compatibility checking")
                suggestions.append("Add fallback capability negotiation")
        
        # General suggestions based on violation type
        if analysis["type"] == "safety_violation":
            suggestions.append("Review invariant conditions for correctness")
            suggestions.append("Add stronger preconditions to prevent invalid states")
        
        elif analysis["type"] == "liveness_violation":
            suggestions.append("Check for missing fairness conditions")
            suggestions.append("Ensure all enabled actions can eventually execute")
        
        return suggestions
    
    @staticmethod
    def generate_debug_script(counterexample: str, config_name: str) -> str:
        """Generate a debugging script to reproduce the counterexample"""
        analysis = CounterexampleAnalyzer.analyze_counterexample(counterexample)
        
        script = f"""#!/usr/bin/env python3
\"\"\"
Debug script for counterexample in configuration: {config_name}
Generated automatically from TLC counterexample trace
\"\"\"

def reproduce_counterexample():
    \"\"\"Reproduce the counterexample scenario\"\"\"
    print("Reproducing counterexample for {config_name}")
    print("Violation type: {analysis['type']}")
    
    if analysis['violated_property']:
        print(f"Violated property: {analysis['violated_property']}")
    
    print("\\nState sequence:")
    """
        
        if analysis["state_sequence"]:
            for i, state in enumerate(analysis["state_sequence"]):
                script += f"""
    print(f"State {i + 1}:")"""
                
                for var, value in state.items():
                    script += f"""
    print(f"  {var}: {value}")"""
        
        script += """

def main():
    reproduce_counterexample()
    
    # Suggested fixes:"""
        
        suggestions = CounterexampleAnalyzer.suggest_fixes(counterexample)
        for suggestion in suggestions:
            script += f"""
    # - {suggestion}"""
        
        script += """

if __name__ == "__main__":
    main()
"""
        
        return script

def main():
    parser = argparse.ArgumentParser(description="Run TLC model checking for Nyx protocol")
    parser.add_argument("--timeout", type=int, default=300, 
                       help="Timeout for each configuration (seconds)")
    parser.add_argument("--java-opts", default="-Xmx4g",
                       help="Java options for TLC")
    parser.add_argument("--output", default="model_checking_report.json",
                       help="Output file for report")
    parser.add_argument("--config-dir", default=".",
                       help="Directory containing configuration files")
    
    args = parser.parse_args()
    
    # Check if TLA+ tools are available
    try:
        subprocess.run(["java", "-cp", "tla2tools.jar", "tlc2.TLC", "-help"], 
                      capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("Error: TLA+ tools not found. Please ensure tla2tools.jar is in the current directory.")
        print("Download from: https://github.com/tlaplus/tlaplus/releases")
        sys.exit(1)
    
    runner = TLCRunner(java_opts=args.java_opts)
    results = runner.run_all_configurations(args.config_dir, args.timeout)
    
    runner.print_summary()
    runner.generate_report(args.output)
    
    # Generate coverage analysis
    coverage_analysis = runner.analyze_all_coverage()
    coverage_output = args.output.replace('.json', '_coverage.json')
    with open(coverage_output, 'w') as f:
        json.dump(coverage_analysis, f, indent=2)
    print(f"Coverage analysis saved to: {coverage_output}")
    
    # Print coverage summary
    print(f"\n📊 COVERAGE ANALYSIS SUMMARY")
    print("=" * 60)
    print(f"Average coverage across all configs: {coverage_analysis['summary']['average_coverage']:.1f}%")
    if coverage_analysis['summary']['best_coverage_config']:
        print(f"Best coverage: {coverage_analysis['summary']['best_coverage_config']}")
    if coverage_analysis['summary']['worst_coverage_config']:
        print(f"Needs improvement: {coverage_analysis['summary']['worst_coverage_config']}")
    
    # Analyze counterexamples if any
    counterexamples_found = [r for r in results if r.counterexample]
    if counterexamples_found:
        print(f"\n🔍 COUNTEREXAMPLE ANALYSIS")
        print("=" * 60)
        
        for result in counterexamples_found:
            print(f"\nConfiguration: {result.config_name}")
            analysis = CounterexampleAnalyzer.analyze_counterexample(result.counterexample)
            print(analysis["analysis"])
    
    # Exit with error code if any configurations failed
    failed_count = sum(1 for r in results if not r.success)
    if failed_count > 0:
        print(f"\n⚠ {failed_count} configuration(s) failed")
        sys.exit(1)
    else:
        print(f"\n✓ All configurations passed successfully")

if __name__ == "__main__":
    main()