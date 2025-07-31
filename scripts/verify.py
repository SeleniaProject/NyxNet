#!/usr/bin/env python3
"""
Automated Verification Pipeline for Nyx Protocol
Integrates TLA+ model checking with Rust property-based tests
and generates comprehensive verification reports.
"""

import os
import sys
import subprocess
import json
import time
import argparse
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any
from dataclasses import dataclass, asdict
import tempfile
import shutil

@dataclass
class VerificationResult:
    """Result of a single verification step"""
    name: str
    type: str  # 'tla', 'rust_test', 'property_test'
    success: bool
    duration: float
    details: Dict[str, Any]
    error_message: Optional[str] = None
    timestamp: str = ""
    
    def __post_init__(self):
        if not self.timestamp:
            self.timestamp = datetime.now().isoformat()

@dataclass
class VerificationReport:
    """Complete verification report"""
    timestamp: str
    total_duration: float
    summary: Dict[str, Any]
    results: List[VerificationResult]
    coverage_metrics: Dict[str, Any]
    
class TLAModelChecker:
    """TLA+ model checking integration"""
    
    def __init__(self, java_opts: str = "-Xmx4g"):
        self.java_opts = java_opts
        self.formal_dir = Path("formal")
        self.tla_tools_jar = self.formal_dir / "tla2tools.jar"
        
    def setup_tla_tools(self) -> bool:
        """Download TLA+ tools if not present"""
        if self.tla_tools_jar.exists():
            return True
            
        print("Downloading TLA+ tools...")
        try:
            import urllib.request
            urllib.request.urlretrieve(
                "https://tla.msr-inria.inria.fr/tlatoolbox/resources/tla2tools.jar",
                str(self.tla_tools_jar)
            )
            return True
        except Exception as e:
            print(f"Failed to download TLA+ tools: {e}")
            return False
    
    def run_model_checking(self, timeout: int = 300) -> List[VerificationResult]:
        """Run comprehensive TLA+ model checking"""
        if not self.setup_tla_tools():
            return [VerificationResult(
                "tla_setup", "tla", False, 0.0, {},
                "Failed to setup TLA+ tools"
            )]
        
        configs = [
            ("basic", 60),
            ("comprehensive", 300), 
            ("scalability", 600),
            ("capability_stress", 400),
            ("liveness_focus", 120)
        ]
        
        results = []
        for config_name, config_timeout in configs:
            config_file = self.formal_dir / f"{config_name}.cfg"
            if not config_file.exists():
                results.append(VerificationResult(
                    f"tla_{config_name}", "tla", False, 0.0, {},
                    f"Configuration file {config_file} not found"
                ))
                continue
                
            result = self._run_single_tlc(config_name, config_file, 
                                        min(timeout, config_timeout))
            results.append(result)
        
        return results
    
    def _run_single_tlc(self, config_name: str, config_file: Path, 
                       timeout: int) -> VerificationResult:
        """Run TLC on a single configuration"""
        tla_file = self.formal_dir / "nyx_multipath_plugin.tla"
        
        cmd = [
            "java", self.java_opts, "-cp", str(self.tla_tools_jar),
            "tlc2.TLC", "-config", str(config_file), str(tla_file)
        ]
        
        start_time = time.time()
        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=timeout,
                cwd=str(self.formal_dir)
            )
            duration = time.time() - start_time
            
            # Parse TLC output
            states_generated, distinct_states = self._parse_tlc_stats(result.stdout)
            success = result.returncode == 0
            
            details = {
                "states_generated": states_generated,
                "distinct_states": distinct_states,
                "return_code": result.returncode,
                "stdout_lines": len(result.stdout.split('\n')),
                "stderr_lines": len(result.stderr.split('\n'))
            }
            
            error_msg = None
            if not success:
                error_msg = self._extract_error_message(result.stdout, result.stderr)
                if "Invariant" in result.stdout or "Temporal property" in result.stdout:
                    details["counterexample"] = self._extract_counterexample(result.stdout)
            
            return VerificationResult(
                f"tla_{config_name}", "tla", success, duration, details, error_msg
            )
            
        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            return VerificationResult(
                f"tla_{config_name}", "tla", False, duration, 
                {"timeout": timeout}, f"Timeout after {timeout}s"
            )
        except Exception as e:
            duration = time.time() - start_time
            return VerificationResult(
                f"tla_{config_name}", "tla", False, duration, {}, str(e)
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
    
    def _extract_counterexample(self, output: str) -> Optional[str]:
        """Extract counterexample from TLC output"""
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

class RustTestRunner:
    """Rust test execution and property-based testing"""
    
    def __init__(self):
        self.workspace_root = Path(".")
        
    def run_conformance_tests(self) -> List[VerificationResult]:
        """Run nyx-conformance property-based tests"""
        conformance_dir = self.workspace_root / "nyx-conformance"
        
        if not conformance_dir.exists():
            return [VerificationResult(
                "conformance_tests", "rust_test", False, 0.0, {},
                "nyx-conformance directory not found"
            )]
        
        # Run different test categories
        test_categories = [
            ("multipath_selection", "multipath_selection_properties"),
            ("capability_negotiation", "capability_negotiation_properties"),
            ("protocol_state_machine", "protocol_state_machine_properties"),
            ("cryptographic_operations", "cryptographic_operation_properties"),
            ("network_simulation", "network_simulation_properties")
        ]
        
        results = []
        for category, test_file in test_categories:
            result = self._run_single_test_file(category, test_file, conformance_dir)
            results.append(result)
        
        return results
    
    def _run_single_test_file(self, category: str, test_file: str, 
                             test_dir: Path) -> VerificationResult:
        """Run tests from a single test file"""
        cmd = [
            "cargo", "test", "--test", test_file, "--", "--nocapture"
        ]
        
        start_time = time.time()
        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=300,
                cwd=str(test_dir)
            )
            duration = time.time() - start_time
            
            # Parse test output
            test_stats = self._parse_test_output(result.stdout)
            success = result.returncode == 0
            
            details = {
                "tests_run": test_stats.get("passed", 0) + test_stats.get("failed", 0),
                "tests_passed": test_stats.get("passed", 0),
                "tests_failed": test_stats.get("failed", 0),
                "return_code": result.returncode
            }
            
            error_msg = None
            if not success:
                error_msg = self._extract_test_failures(result.stdout)
            
            return VerificationResult(
                f"rust_{category}", "rust_test", success, duration, details, error_msg
            )
            
        except subprocess.TimeoutExpired:
            duration = time.time() - start_time
            return VerificationResult(
                f"rust_{category}", "rust_test", False, duration,
                {"timeout": 300}, "Test timeout after 300s"
            )
        except Exception as e:
            duration = time.time() - start_time
            return VerificationResult(
                f"rust_{category}", "rust_test", False, duration, {}, str(e)
            )
    
    def _parse_test_output(self, output: str) -> Dict[str, int]:
        """Parse Rust test output for statistics"""
        stats = {"passed": 0, "failed": 0, "ignored": 0}
        
        for line in output.split('\n'):
            if "test result:" in line:
                # Example: "test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
                parts = line.split()
                for i, part in enumerate(parts):
                    if part == "passed;" and i > 0:
                        try:
                            stats["passed"] = int(parts[i-1])
                        except (ValueError, IndexError):
                            pass
                    elif part == "failed;" and i > 0:
                        try:
                            stats["failed"] = int(parts[i-1])
                        except (ValueError, IndexError):
                            pass
                    elif part == "ignored;" and i > 0:
                        try:
                            stats["ignored"] = int(parts[i-1])
                        except (ValueError, IndexError):
                            pass
        
        return stats
    
    def _extract_test_failures(self, output: str) -> Optional[str]:
        """Extract test failure information"""
        lines = output.split('\n')
        failure_lines = []
        in_failure = False
        
        for line in lines:
            if "FAILED" in line or "panicked at" in line:
                in_failure = True
                failure_lines.append(line)
            elif in_failure and (line.startswith("thread") or line.startswith("note:")):
                failure_lines.append(line)
            elif in_failure and line.strip() == "":
                if len(failure_lines) > 5:  # Enough context
                    break
        
        return '\n'.join(failure_lines) if failure_lines else None

class CoverageAnalyzer:
    """Analyze verification coverage metrics"""
    
    def __init__(self):
        self.formal_dir = Path("formal")
        self.conformance_dir = Path("nyx-conformance")
    
    def analyze_coverage(self, results: List[VerificationResult]) -> Dict[str, Any]:
        """Analyze verification coverage across TLA+ and Rust tests"""
        tla_results = [r for r in results if r.type == "tla"]
        rust_results = [r for r in results if r.type == "rust_test"]
        
        coverage = {
            "tla_coverage": self._analyze_tla_coverage(tla_results),
            "rust_coverage": self._analyze_rust_coverage(rust_results),
            "integration_coverage": self._analyze_integration_coverage(results),
            "requirements_coverage": self._analyze_requirements_coverage(results)
        }
        
        return coverage
    
    def _analyze_tla_coverage(self, tla_results: List[VerificationResult]) -> Dict[str, Any]:
        """Analyze TLA+ model checking coverage"""
        successful_configs = [r for r in tla_results if r.success]
        total_states = sum(r.details.get("states_generated", 0) for r in successful_configs)
        
        return {
            "configurations_tested": len(tla_results),
            "configurations_passed": len(successful_configs),
            "total_states_explored": total_states,
            "coverage_percentage": (len(successful_configs) / len(tla_results) * 100) if tla_results else 0,
            "properties_verified": self._count_verified_properties(successful_configs)
        }
    
    def _analyze_rust_coverage(self, rust_results: List[VerificationResult]) -> Dict[str, Any]:
        """Analyze Rust property-based test coverage"""
        successful_tests = [r for r in rust_results if r.success]
        total_tests = sum(r.details.get("tests_run", 0) for r in rust_results)
        passed_tests = sum(r.details.get("tests_passed", 0) for r in rust_results)
        
        return {
            "test_categories": len(rust_results),
            "categories_passed": len(successful_tests),
            "total_property_tests": total_tests,
            "passed_property_tests": passed_tests,
            "test_success_rate": (passed_tests / total_tests * 100) if total_tests > 0 else 0
        }
    
    def _analyze_integration_coverage(self, results: List[VerificationResult]) -> Dict[str, Any]:
        """Analyze integration between TLA+ and Rust tests"""
        # Map TLA+ properties to Rust test categories
        property_mapping = {
            "multipath_selection": ["tla_basic", "tla_comprehensive", "rust_multipath_selection"],
            "capability_negotiation": ["tla_capability_stress", "rust_capability_negotiation"],
            "state_machine": ["tla_liveness_focus", "rust_protocol_state_machine"],
            "cryptographic": ["rust_cryptographic_operations"],
            "network_behavior": ["tla_scalability", "rust_network_simulation"]
        }
        
        integration_coverage = {}
        for property_name, required_tests in property_mapping.items():
            covered_tests = [r.name for r in results if r.name in required_tests and r.success]
            integration_coverage[property_name] = {
                "required_tests": required_tests,
                "covered_tests": covered_tests,
                "coverage_percentage": (len(covered_tests) / len(required_tests) * 100)
            }
        
        return integration_coverage
    
    def _analyze_requirements_coverage(self, results: List[VerificationResult]) -> Dict[str, Any]:
        """Map verification results to requirements"""
        # Based on requirements from the spec
        requirements_mapping = {
            "1.1": ["tla_basic", "tla_comprehensive"],  # Safety invariants
            "1.2": ["tla_capability_stress", "rust_capability_negotiation"],  # Capability consistency
            "1.3": ["tla_basic", "rust_multipath_selection"],  # Path uniqueness
            "1.4": ["tla_liveness_focus", "rust_protocol_state_machine"],  # State transitions
            "1.5": ["tla_liveness_focus"],  # Liveness properties
            "2.1": ["tla_comprehensive"],  # Complete state space
            "2.2": ["tla_scalability"],  # Scalability testing
            "2.3": ["tla_capability_stress"],  # Capability combinations
            "2.4": ["tla_basic", "tla_comprehensive"],  # Path length validation
            "2.5": ["tla_liveness_focus"],  # Temporal properties
            "2.6": ["tla_basic", "tla_comprehensive", "tla_scalability"],  # Counterexample documentation
            "3.1": ["rust_multipath_selection"],  # Path generation tests
            "3.2": ["rust_capability_negotiation"],  # Capability negotiation tests
            "3.3": ["rust_protocol_state_machine"],  # State machine tests
            "3.4": ["rust_cryptographic_operations"],  # Cryptographic tests
            "3.5": ["rust_network_simulation"],  # Network simulation tests
            "3.6": ["rust_capability_negotiation", "rust_network_simulation"],  # Error handling
            "3.7": ["rust_multipath_selection", "rust_capability_negotiation", 
                   "rust_protocol_state_machine", "rust_network_simulation"]  # Property violations
        }
        
        coverage_by_requirement = {}
        for req_id, required_tests in requirements_mapping.items():
            covered_tests = [r.name for r in results if r.name in required_tests and r.success]
            coverage_by_requirement[req_id] = {
                "required_tests": required_tests,
                "covered_tests": covered_tests,
                "coverage_percentage": (len(covered_tests) / len(required_tests) * 100),
                "fully_covered": len(covered_tests) == len(required_tests)
            }
        
        total_requirements = len(requirements_mapping)
        fully_covered = sum(1 for cov in coverage_by_requirement.values() if cov["fully_covered"])
        
        return {
            "requirements_mapping": coverage_by_requirement,
            "total_requirements": total_requirements,
            "fully_covered_requirements": fully_covered,
            "overall_coverage_percentage": (fully_covered / total_requirements * 100)
        }
    
    def _count_verified_properties(self, successful_configs: List[VerificationResult]) -> Dict[str, int]:
        """Count verified properties by type"""
        # This would need to parse TLA+ output more thoroughly
        # For now, return estimated counts based on successful configurations
        property_counts = {
            "safety_invariants": 0,
            "liveness_properties": 0,
            "temporal_properties": 0
        }
        
        for result in successful_configs:
            config_name = result.name.replace("tla_", "")
            if config_name in ["basic", "comprehensive", "scalability"]:
                property_counts["safety_invariants"] += 6  # Estimated invariants
            if config_name in ["liveness_focus", "comprehensive"]:
                property_counts["liveness_properties"] += 3  # Estimated liveness properties
            if config_name == "liveness_focus":
                property_counts["temporal_properties"] += 2  # Estimated temporal properties
        
        return property_counts

class VerificationPipeline:
    """Main verification pipeline orchestrator"""
    
    def __init__(self, java_opts: str = "-Xmx4g", timeout: int = 600):
        self.tla_checker = TLAModelChecker(java_opts)
        self.rust_runner = RustTestRunner()
        self.coverage_analyzer = CoverageAnalyzer()
        self.timeout = timeout
        
    def run_full_verification(self) -> VerificationReport:
        """Run complete verification pipeline"""
        print("Starting Nyx Protocol Verification Pipeline")
        print("=" * 60)
        
        start_time = time.time()
        all_results = []
        
        # Run TLA+ model checking
        print("\n🔍 Running TLA+ Model Checking...")
        tla_results = self.tla_checker.run_model_checking(self.timeout)
        all_results.extend(tla_results)
        
        # Run Rust property-based tests
        print("\n🦀 Running Rust Property-Based Tests...")
        rust_results = self.rust_runner.run_conformance_tests()
        all_results.extend(rust_results)
        
        # Analyze coverage
        print("\n📊 Analyzing Verification Coverage...")
        coverage_metrics = self.coverage_analyzer.analyze_coverage(all_results)
        
        total_duration = time.time() - start_time
        
        # Generate summary
        successful_results = [r for r in all_results if r.success]
        summary = {
            "total_verifications": len(all_results),
            "successful_verifications": len(successful_results),
            "failed_verifications": len(all_results) - len(successful_results),
            "success_rate": (len(successful_results) / len(all_results) * 100) if all_results else 0,
            "tla_verifications": len([r for r in all_results if r.type == "tla"]),
            "rust_verifications": len([r for r in all_results if r.type == "rust_test"]),
            "total_duration_seconds": total_duration
        }
        
        return VerificationReport(
            timestamp=datetime.now().isoformat(),
            total_duration=total_duration,
            summary=summary,
            results=all_results,
            coverage_metrics=coverage_metrics
        )
    
    def generate_report(self, report: VerificationReport, output_file: str) -> None:
        """Generate comprehensive verification report"""
        report_dict = {
            "timestamp": report.timestamp,
            "total_duration": report.total_duration,
            "summary": report.summary,
            "results": [asdict(r) for r in report.results],
            "coverage_metrics": report.coverage_metrics
        }
        
        with open(output_file, 'w') as f:
            json.dump(report_dict, f, indent=2)
        
        print(f"\n📄 Verification report saved to: {output_file}")
    
    def print_summary(self, report: VerificationReport) -> None:
        """Print verification summary to console"""
        print("\n" + "=" * 60)
        print("VERIFICATION PIPELINE SUMMARY")
        print("=" * 60)
        
        summary = report.summary
        print(f"Total Duration: {summary['total_duration_seconds']:.2f}s")
        print(f"Success Rate: {summary['success_rate']:.1f}%")
        print(f"Verifications: {summary['successful_verifications']}/{summary['total_verifications']}")
        
        # TLA+ Results
        tla_results = [r for r in report.results if r.type == "tla"]
        tla_success = [r for r in tla_results if r.success]
        print(f"\n🔍 TLA+ Model Checking: {len(tla_success)}/{len(tla_results)} passed")
        
        for result in tla_results:
            status = "✓" if result.success else "✗"
            states = result.details.get("states_generated", 0)
            print(f"  {status} {result.name}: {states:,} states in {result.duration:.2f}s")
        
        # Rust Results
        rust_results = [r for r in report.results if r.type == "rust_test"]
        rust_success = [r for r in rust_results if r.success]
        print(f"\n🦀 Rust Property Tests: {len(rust_success)}/{len(rust_results)} passed")
        
        for result in rust_results:
            status = "✓" if result.success else "✗"
            tests = result.details.get("tests_run", 0)
            print(f"  {status} {result.name}: {tests} tests in {result.duration:.2f}s")
        
        # Coverage Summary
        coverage = report.coverage_metrics
        req_coverage = coverage.get("requirements_coverage", {})
        overall_coverage = req_coverage.get("overall_coverage_percentage", 0)
        print(f"\n📊 Requirements Coverage: {overall_coverage:.1f}%")
        
        # Failed verifications
        failed_results = [r for r in report.results if not r.success]
        if failed_results:
            print(f"\n❌ Failed Verifications:")
            for result in failed_results:
                print(f"  {result.name}: {result.error_message or 'Unknown error'}")

def main():
    parser = argparse.ArgumentParser(description="Nyx Protocol Verification Pipeline")
    parser.add_argument("--timeout", type=int, default=600,
                       help="Timeout for verification steps (seconds)")
    parser.add_argument("--java-opts", default="-Xmx4g",
                       help="Java options for TLA+ model checking")
    parser.add_argument("--output", default="verification_report.json",
                       help="Output file for verification report")
    parser.add_argument("--tla-only", action="store_true",
                       help="Run only TLA+ model checking")
    parser.add_argument("--rust-only", action="store_true",
                       help="Run only Rust property tests")
    
    args = parser.parse_args()
    
    pipeline = VerificationPipeline(args.java_opts, args.timeout)
    
    if args.tla_only:
        print("Running TLA+ model checking only...")
        results = pipeline.tla_checker.run_model_checking(args.timeout)
        # Create minimal report for TLA+ only
        report = VerificationReport(
            timestamp=datetime.now().isoformat(),
            total_duration=sum(r.duration for r in results),
            summary={"tla_only": True},
            results=results,
            coverage_metrics={}
        )
    elif args.rust_only:
        print("Running Rust property tests only...")
        results = pipeline.rust_runner.run_conformance_tests()
        # Create minimal report for Rust only
        report = VerificationReport(
            timestamp=datetime.now().isoformat(),
            total_duration=sum(r.duration for r in results),
            summary={"rust_only": True},
            results=results,
            coverage_metrics={}
        )
    else:
        report = pipeline.run_full_verification()
    
    pipeline.print_summary(report)
    pipeline.generate_report(report, args.output)
    
    # Exit with error code if any verifications failed
    failed_count = len([r for r in report.results if not r.success])
    if failed_count > 0:
        print(f"\n⚠ {failed_count} verification(s) failed")
        sys.exit(1)
    else:
        print(f"\n✅ All verifications passed successfully")

if __name__ == "__main__":
    main()