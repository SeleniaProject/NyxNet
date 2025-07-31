#!/usr/bin/env python3
"""
Verification Report Generator
Creates comprehensive reports for formal verification results
with coverage metrics and requirement traceability.
"""

import json
import os
import sys
import argparse
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Any, Optional
import subprocess

class VerificationReportGenerator:
    """Generate comprehensive verification reports with coverage metrics"""
    
    def __init__(self, project_root: str = "."):
        self.project_root = Path(project_root)
        self.formal_dir = self.project_root / "formal"
        self.conformance_dir = self.project_root / "nyx-conformance"
        
    def generate_comprehensive_report(self, verification_report_path: str, 
                                    output_path: str = "verification_coverage_report.json") -> Dict[str, Any]:
        """Generate comprehensive verification report with coverage metrics"""
        
        # Load verification results
        verification_data = self._load_verification_results(verification_report_path)
        
        # Analyze code coverage
        code_coverage = self._analyze_code_coverage()
        
        # Analyze TLA+ model coverage
        tla_coverage = self._analyze_tla_coverage(verification_data)
        
        # Analyze requirements traceability
        requirements_traceability = self._analyze_requirements_traceability(verification_data)
        
        # Generate test matrix coverage
        test_matrix = self._generate_test_matrix(verification_data)
        
        # Calculate overall metrics
        overall_metrics = self._calculate_overall_metrics(
            verification_data, code_coverage, tla_coverage, requirements_traceability
        )
        
        # Create comprehensive report
        comprehensive_report = {
            "metadata": {
                "generated_at": datetime.now().isoformat(),
                "generator_version": "1.0.0",
                "project_root": str(self.project_root),
                "verification_report_source": verification_report_path
            },
            "overall_metrics": overall_metrics,
            "verification_results": verification_data,
            "code_coverage": code_coverage,
            "tla_coverage": tla_coverage,
            "requirements_traceability": requirements_traceability,
            "test_matrix": test_matrix,
            "recommendations": self._generate_recommendations(overall_metrics)
        }
        
        # Save report
        with open(output_path, 'w') as f:
            json.dump(comprehensive_report, f, indent=2)
        
        return comprehensive_report
    
    def _load_verification_results(self, report_path: str) -> Dict[str, Any]:
        """Load verification results from JSON report"""
        try:
            with open(report_path, 'r') as f:
                return json.load(f)
        except FileNotFoundError:
            print(f"Warning: Verification report not found at {report_path}")
            return {}
        except json.JSONDecodeError as e:
            print(f"Error parsing verification report: {e}")
            return {}
    
    def _analyze_code_coverage(self) -> Dict[str, Any]:
        """Analyze Rust code coverage for conformance tests"""
        coverage_data = {
            "tool": "cargo-tarpaulin",
            "coverage_percentage": 0.0,
            "lines_covered": 0,
            "lines_total": 0,
            "files_analyzed": [],
            "uncovered_areas": []
        }
        
        # Try to run cargo-tarpaulin if available
        try:
            result = subprocess.run([
                "cargo", "tarpaulin", "--workspace", "--out", "Json", 
                "--exclude-files", "target/*", "--timeout", "300"
            ], capture_output=True, text=True, cwd=str(self.project_root))
            
            if result.returncode == 0:
                tarpaulin_data = json.loads(result.stdout)
                coverage_data.update({
                    "coverage_percentage": tarpaulin_data.get("coverage", 0.0),
                    "lines_covered": tarpaulin_data.get("covered", 0),
                    "lines_total": tarpaulin_data.get("coverable", 0),
                    "files_analyzed": [f["name"] for f in tarpaulin_data.get("files", [])],
                    "raw_data": tarpaulin_data
                })
        except (subprocess.SubprocessError, json.JSONDecodeError, FileNotFoundError):
            # Fallback to basic analysis
            coverage_data["note"] = "cargo-tarpaulin not available, using basic analysis"
            coverage_data.update(self._basic_coverage_analysis())
        
        return coverage_data
    
    def _basic_coverage_analysis(self) -> Dict[str, Any]:
        """Basic coverage analysis without external tools"""
        test_files = list(self.conformance_dir.glob("tests/**/*.rs"))
        src_files = list(self.conformance_dir.glob("src/**/*.rs"))
        
        return {
            "test_files_count": len(test_files),
            "source_files_count": len(src_files),
            "estimated_coverage": min(85.0, len(test_files) * 15.0),  # Rough estimate
            "test_files": [str(f.relative_to(self.project_root)) for f in test_files],
            "source_files": [str(f.relative_to(self.project_root)) for f in src_files]
        }
    
    def _analyze_tla_coverage(self, verification_data: Dict[str, Any]) -> Dict[str, Any]:
        """Analyze TLA+ model coverage"""
        tla_coverage = {
            "model_file": "formal/nyx_multipath_plugin.tla",
            "configurations_tested": 0,
            "configurations_passed": 0,
            "total_states_explored": 0,
            "invariants_verified": [],
            "liveness_properties_verified": [],
            "configuration_details": {}
        }
        
        # Extract TLA+ results from verification data
        results = verification_data.get("results", [])
        tla_results = [r for r in results if r.get("type") == "tla"]
        
        tla_coverage["configurations_tested"] = len(tla_results)
        tla_coverage["configurations_passed"] = len([r for r in tla_results if r.get("success")])
        
        for result in tla_results:
            config_name = result.get("name", "").replace("tla_", "")
            details = result.get("details", {})
            
            tla_coverage["total_states_explored"] += details.get("states_generated", 0)
            tla_coverage["configuration_details"][config_name] = {
                "success": result.get("success", False),
                "duration": result.get("duration", 0),
                "states_generated": details.get("states_generated", 0),
                "distinct_states": details.get("distinct_states", 0)
            }
            
            # Infer verified properties based on successful configurations
            if result.get("success"):
                if config_name in ["basic", "comprehensive", "scalability"]:
                    tla_coverage["invariants_verified"].extend([
                        "TypeInvariant", "Inv_PathLen", "Inv_NoDup", 
                        "Inv_Error", "Inv_NoError", "Inv_PowerState"
                    ])
                if config_name in ["liveness_focus", "comprehensive"]:
                    tla_coverage["liveness_properties_verified"].extend([
                        "Terminating", "ProgressToOpen", "ProgressToClose"
                    ])
        
        # Remove duplicates
        tla_coverage["invariants_verified"] = list(set(tla_coverage["invariants_verified"]))
        tla_coverage["liveness_properties_verified"] = list(set(tla_coverage["liveness_properties_verified"]))
        
        # Calculate coverage percentage
        total_properties = len(tla_coverage["invariants_verified"]) + len(tla_coverage["liveness_properties_verified"])
        expected_properties = 9  # Expected total properties in the model
        tla_coverage["property_coverage_percentage"] = min(100.0, (total_properties / expected_properties) * 100)
        
        return tla_coverage
    
    def _analyze_requirements_traceability(self, verification_data: Dict[str, Any]) -> Dict[str, Any]:
        """Analyze requirements traceability and coverage"""
        
        # Requirements mapping from the spec
        requirements_mapping = {
            "1.1": {
                "description": "Safety invariants formally proven",
                "verification_methods": ["tla_basic", "tla_comprehensive"],
                "category": "safety_proofs"
            },
            "1.2": {
                "description": "Capability negotiation consistency",
                "verification_methods": ["tla_capability_stress", "rust_capability_negotiation"],
                "category": "safety_proofs"
            },
            "1.3": {
                "description": "Path uniqueness guaranteed",
                "verification_methods": ["tla_basic", "rust_multipath_selection"],
                "category": "safety_proofs"
            },
            "1.4": {
                "description": "State transitions maintain validity",
                "verification_methods": ["tla_liveness_focus", "rust_protocol_state_machine"],
                "category": "safety_proofs"
            },
            "1.5": {
                "description": "Liveness properties verified",
                "verification_methods": ["tla_liveness_focus"],
                "category": "safety_proofs"
            },
            "2.1": {
                "description": "Complete state space verification",
                "verification_methods": ["tla_comprehensive"],
                "category": "model_checking"
            },
            "2.2": {
                "description": "Scalability testing",
                "verification_methods": ["tla_scalability"],
                "category": "model_checking"
            },
            "2.3": {
                "description": "Capability combinations tested",
                "verification_methods": ["tla_capability_stress"],
                "category": "model_checking"
            },
            "2.4": {
                "description": "Path length validation",
                "verification_methods": ["tla_basic", "tla_comprehensive"],
                "category": "model_checking"
            },
            "2.5": {
                "description": "Temporal properties checked",
                "verification_methods": ["tla_liveness_focus"],
                "category": "model_checking"
            },
            "2.6": {
                "description": "Counterexample documentation",
                "verification_methods": ["tla_basic", "tla_comprehensive", "tla_scalability"],
                "category": "model_checking"
            },
            "3.1": {
                "description": "Multipath selection property tests",
                "verification_methods": ["rust_multipath_selection"],
                "category": "property_testing"
            },
            "3.2": {
                "description": "Capability negotiation property tests",
                "verification_methods": ["rust_capability_negotiation"],
                "category": "property_testing"
            },
            "3.3": {
                "description": "Protocol state machine property tests",
                "verification_methods": ["rust_protocol_state_machine"],
                "category": "property_testing"
            },
            "3.4": {
                "description": "Cryptographic operation property tests",
                "verification_methods": ["rust_cryptographic_operations"],
                "category": "property_testing"
            },
            "3.5": {
                "description": "Network simulation property tests",
                "verification_methods": ["rust_network_simulation"],
                "category": "property_testing"
            },
            "3.6": {
                "description": "Error handling verification",
                "verification_methods": ["rust_capability_negotiation", "rust_network_simulation"],
                "category": "property_testing"
            },
            "3.7": {
                "description": "Property violation detection",
                "verification_methods": ["rust_multipath_selection", "rust_capability_negotiation", 
                                       "rust_protocol_state_machine", "rust_network_simulation"],
                "category": "property_testing"
            }
        }
        
        # Analyze coverage for each requirement
        results = verification_data.get("results", [])
        successful_verifications = {r.get("name"): r.get("success", False) for r in results}
        
        requirement_coverage = {}
        for req_id, req_info in requirements_mapping.items():
            required_methods = req_info["verification_methods"]
            covered_methods = [method for method in required_methods 
                             if successful_verifications.get(method, False)]
            
            coverage_percentage = (len(covered_methods) / len(required_methods)) * 100
            
            requirement_coverage[req_id] = {
                "description": req_info["description"],
                "category": req_info["category"],
                "required_methods": required_methods,
                "covered_methods": covered_methods,
                "coverage_percentage": coverage_percentage,
                "fully_covered": len(covered_methods) == len(required_methods),
                "status": "✅ Covered" if len(covered_methods) == len(required_methods) else 
                         "⚠️ Partial" if covered_methods else "❌ Not Covered"
            }
        
        # Calculate category summaries
        categories = {}
        for req_id, req_coverage in requirement_coverage.items():
            category = req_coverage["category"]
            if category not in categories:
                categories[category] = {
                    "total_requirements": 0,
                    "fully_covered": 0,
                    "partially_covered": 0,
                    "not_covered": 0
                }
            
            categories[category]["total_requirements"] += 1
            if req_coverage["fully_covered"]:
                categories[category]["fully_covered"] += 1
            elif req_coverage["covered_methods"]:
                categories[category]["partially_covered"] += 1
            else:
                categories[category]["not_covered"] += 1
        
        # Calculate overall coverage
        total_requirements = len(requirement_coverage)
        fully_covered = sum(1 for r in requirement_coverage.values() if r["fully_covered"])
        overall_coverage_percentage = (fully_covered / total_requirements) * 100
        
        return {
            "overall_coverage_percentage": overall_coverage_percentage,
            "total_requirements": total_requirements,
            "fully_covered_requirements": fully_covered,
            "partially_covered_requirements": sum(1 for r in requirement_coverage.values() 
                                                if r["covered_methods"] and not r["fully_covered"]),
            "not_covered_requirements": sum(1 for r in requirement_coverage.values() 
                                          if not r["covered_methods"]),
            "requirement_details": requirement_coverage,
            "category_summary": categories
        }
    
    def _generate_test_matrix(self, verification_data: Dict[str, Any]) -> Dict[str, Any]:
        """Generate test matrix showing verification coverage"""
        
        # Define test matrix dimensions
        verification_types = ["TLA+ Model Checking", "Rust Property Tests"]
        protocol_aspects = [
            "Multipath Selection", "Capability Negotiation", "State Machine",
            "Cryptographic Operations", "Network Simulation", "Error Handling"
        ]
        
        # Map verification results to matrix
        results = verification_data.get("results", [])
        matrix = {}
        
        for aspect in protocol_aspects:
            matrix[aspect] = {}
            for v_type in verification_types:
                matrix[aspect][v_type] = {"status": "Not Tested", "details": []}
        
        # Fill matrix based on results
        for result in results:
            name = result.get("name", "")
            success = result.get("success", False)
            
            # Map result to matrix cells
            if name.startswith("tla_"):
                v_type = "TLA+ Model Checking"
                if "multipath" in name or "basic" in name or "comprehensive" in name:
                    matrix["Multipath Selection"][v_type] = {
                        "status": "✅ Passed" if success else "❌ Failed",
                        "details": [f"{name}: {result.get('duration', 0):.2f}s"]
                    }
                if "capability" in name:
                    matrix["Capability Negotiation"][v_type] = {
                        "status": "✅ Passed" if success else "❌ Failed",
                        "details": [f"{name}: {result.get('duration', 0):.2f}s"]
                    }
                if "liveness" in name or "comprehensive" in name:
                    matrix["State Machine"][v_type] = {
                        "status": "✅ Passed" if success else "❌ Failed",
                        "details": [f"{name}: {result.get('duration', 0):.2f}s"]
                    }
            
            elif name.startswith("rust_"):
                v_type = "Rust Property Tests"
                aspect_map = {
                    "multipath_selection": "Multipath Selection",
                    "capability_negotiation": "Capability Negotiation",
                    "protocol_state_machine": "State Machine",
                    "cryptographic_operations": "Cryptographic Operations",
                    "network_simulation": "Network Simulation"
                }
                
                for key, aspect in aspect_map.items():
                    if key in name:
                        tests_run = result.get("details", {}).get("tests_run", 0)
                        matrix[aspect][v_type] = {
                            "status": "✅ Passed" if success else "❌ Failed",
                            "details": [f"{name}: {tests_run} tests in {result.get('duration', 0):.2f}s"]
                        }
                        
                        # Error handling is covered by multiple test types
                        if key in ["capability_negotiation", "network_simulation"]:
                            if matrix["Error Handling"][v_type]["status"] == "Not Tested":
                                matrix["Error Handling"][v_type] = {
                                    "status": "✅ Passed" if success else "❌ Failed",
                                    "details": []
                                }
                            matrix["Error Handling"][v_type]["details"].append(
                                f"{name}: error handling tests"
                            )
        
        return {
            "verification_types": verification_types,
            "protocol_aspects": protocol_aspects,
            "matrix": matrix,
            "coverage_summary": self._calculate_matrix_coverage(matrix)
        }
    
    def _calculate_matrix_coverage(self, matrix: Dict[str, Any]) -> Dict[str, Any]:
        """Calculate coverage summary for test matrix"""
        total_cells = 0
        tested_cells = 0
        passed_cells = 0
        
        for aspect, v_types in matrix.items():
            for v_type, cell in v_types.items():
                total_cells += 1
                if cell["status"] != "Not Tested":
                    tested_cells += 1
                    if "✅" in cell["status"]:
                        passed_cells += 1
        
        return {
            "total_cells": total_cells,
            "tested_cells": tested_cells,
            "passed_cells": passed_cells,
            "test_coverage_percentage": (tested_cells / total_cells) * 100,
            "pass_rate_percentage": (passed_cells / tested_cells) * 100 if tested_cells > 0 else 0
        }
    
    def _calculate_overall_metrics(self, verification_data: Dict[str, Any], 
                                 code_coverage: Dict[str, Any], 
                                 tla_coverage: Dict[str, Any],
                                 requirements_traceability: Dict[str, Any]) -> Dict[str, Any]:
        """Calculate overall verification metrics"""
        
        # Verification success metrics
        results = verification_data.get("results", [])
        total_verifications = len(results)
        successful_verifications = len([r for r in results if r.get("success")])
        verification_success_rate = (successful_verifications / total_verifications) * 100 if total_verifications > 0 else 0
        
        # Coverage metrics
        code_coverage_pct = code_coverage.get("coverage_percentage", 0)
        tla_coverage_pct = tla_coverage.get("property_coverage_percentage", 0)
        requirements_coverage_pct = requirements_traceability.get("overall_coverage_percentage", 0)
        
        # Calculate composite score
        composite_score = (
            verification_success_rate * 0.4 +  # 40% weight on verification success
            requirements_coverage_pct * 0.3 +  # 30% weight on requirements coverage
            tla_coverage_pct * 0.2 +           # 20% weight on TLA+ coverage
            min(code_coverage_pct, 100) * 0.1  # 10% weight on code coverage
        )
        
        return {
            "composite_score": composite_score,
            "verification_success_rate": verification_success_rate,
            "code_coverage_percentage": code_coverage_pct,
            "tla_coverage_percentage": tla_coverage_pct,
            "requirements_coverage_percentage": requirements_coverage_pct,
            "total_verifications": total_verifications,
            "successful_verifications": successful_verifications,
            "grade": self._calculate_grade(composite_score),
            "status": "✅ Excellent" if composite_score >= 90 else
                     "⚠️ Good" if composite_score >= 80 else
                     "❌ Needs Improvement"
        }
    
    def _calculate_grade(self, score: float) -> str:
        """Calculate letter grade based on composite score"""
        if score >= 95:
            return "A+"
        elif score >= 90:
            return "A"
        elif score >= 85:
            return "A-"
        elif score >= 80:
            return "B+"
        elif score >= 75:
            return "B"
        elif score >= 70:
            return "B-"
        elif score >= 65:
            return "C+"
        elif score >= 60:
            return "C"
        else:
            return "F"
    
    def _generate_recommendations(self, overall_metrics: Dict[str, Any]) -> List[str]:
        """Generate recommendations based on metrics"""
        recommendations = []
        
        score = overall_metrics["composite_score"]
        verification_rate = overall_metrics["verification_success_rate"]
        requirements_coverage = overall_metrics["requirements_coverage_percentage"]
        tla_coverage = overall_metrics["tla_coverage_percentage"]
        code_coverage = overall_metrics["code_coverage_percentage"]
        
        if score < 80:
            recommendations.append("🎯 Overall verification score is below 80%. Focus on improving verification success rates and coverage.")
        
        if verification_rate < 90:
            recommendations.append("🔧 Verification success rate is below 90%. Review and fix failing verifications.")
        
        if requirements_coverage < 85:
            recommendations.append("📋 Requirements coverage is below 85%. Add more verification methods for uncovered requirements.")
        
        if tla_coverage < 80:
            recommendations.append("🔍 TLA+ model coverage is below 80%. Consider adding more model checking configurations or properties.")
        
        if code_coverage < 70:
            recommendations.append("🧪 Code coverage is below 70%. Add more unit tests and property-based tests.")
        
        if score >= 90:
            recommendations.append("✨ Excellent verification coverage! Consider this a gold standard for formal verification.")
        elif score >= 80:
            recommendations.append("👍 Good verification coverage. Minor improvements could push this to excellent.")
        
        # Specific recommendations based on missing areas
        if tla_coverage < 100:
            recommendations.append("🎲 Consider adding more TLA+ configurations to test edge cases and larger state spaces.")
        
        if requirements_coverage < 100:
            recommendations.append("🎯 Some requirements are not fully covered. Review the requirements traceability matrix.")
        
        return recommendations
    
    def generate_html_report(self, json_report_path: str, output_path: str = "verification_report.html") -> None:
        """Generate HTML report from JSON data"""
        
        with open(json_report_path, 'r') as f:
            report_data = json.load(f)
        
        html_template = """
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Nyx Protocol Verification Report</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #f5f5f5; }
        .container { max-width: 1200px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
        h1 { color: #2c3e50; border-bottom: 3px solid #3498db; padding-bottom: 10px; }
        h2 { color: #34495e; margin-top: 30px; }
        .metric-card { background: #ecf0f1; padding: 20px; margin: 10px 0; border-radius: 6px; border-left: 4px solid #3498db; }
        .success { border-left-color: #27ae60; }
        .warning { border-left-color: #f39c12; }
        .error { border-left-color: #e74c3c; }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; }
        table { width: 100%; border-collapse: collapse; margin: 20px 0; }
        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background-color: #f8f9fa; font-weight: 600; }
        .status-pass { color: #27ae60; font-weight: bold; }
        .status-fail { color: #e74c3c; font-weight: bold; }
        .status-partial { color: #f39c12; font-weight: bold; }
        .progress-bar { width: 100%; height: 20px; background: #ecf0f1; border-radius: 10px; overflow: hidden; }
        .progress-fill { height: 100%; background: linear-gradient(90deg, #3498db, #2ecc71); transition: width 0.3s ease; }
        .recommendations { background: #fff3cd; border: 1px solid #ffeaa7; border-radius: 6px; padding: 20px; margin: 20px 0; }
        .recommendations ul { margin: 0; padding-left: 20px; }
        .timestamp { color: #7f8c8d; font-size: 0.9em; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🔍 Nyx Protocol Formal Verification Report</h1>
        <p class="timestamp">Generated: {timestamp}</p>
        
        <div class="grid">
            <div class="metric-card {overall_status_class}">
                <h3>Overall Score</h3>
                <div style="font-size: 2em; font-weight: bold;">{composite_score:.1f}% ({grade})</div>
                <div>{status}</div>
            </div>
            
            <div class="metric-card">
                <h3>Verification Success Rate</h3>
                <div class="progress-bar">
                    <div class="progress-fill" style="width: {verification_success_rate:.1f}%"></div>
                </div>
                <div>{successful_verifications}/{total_verifications} verifications passed</div>
            </div>
            
            <div class="metric-card">
                <h3>Requirements Coverage</h3>
                <div class="progress-bar">
                    <div class="progress-fill" style="width: {requirements_coverage_percentage:.1f}%"></div>
                </div>
                <div>{requirements_coverage_percentage:.1f}% requirements covered</div>
            </div>
            
            <div class="metric-card">
                <h3>TLA+ Model Coverage</h3>
                <div class="progress-bar">
                    <div class="progress-fill" style="width: {tla_coverage_percentage:.1f}%"></div>
                </div>
                <div>{tla_coverage_percentage:.1f}% properties verified</div>
            </div>
        </div>
        
        <h2>📊 Verification Results</h2>
        <table>
            <thead>
                <tr>
                    <th>Verification</th>
                    <th>Type</th>
                    <th>Status</th>
                    <th>Duration</th>
                    <th>Details</th>
                </tr>
            </thead>
            <tbody>
                {verification_results_rows}
            </tbody>
        </table>
        
        <h2>📋 Requirements Traceability</h2>
        <table>
            <thead>
                <tr>
                    <th>Requirement</th>
                    <th>Description</th>
                    <th>Status</th>
                    <th>Coverage</th>
                </tr>
            </thead>
            <tbody>
                {requirements_rows}
            </tbody>
        </table>
        
        <div class="recommendations">
            <h2>💡 Recommendations</h2>
            <ul>
                {recommendations_list}
            </ul>
        </div>
        
        <h2>🧪 Test Matrix</h2>
        <table>
            <thead>
                <tr>
                    <th>Protocol Aspect</th>
                    <th>TLA+ Model Checking</th>
                    <th>Rust Property Tests</th>
                </tr>
            </thead>
            <tbody>
                {test_matrix_rows}
            </tbody>
        </table>
    </div>
</body>
</html>
"""
        
        # Extract data for template
        overall_metrics = report_data["overall_metrics"]
        verification_results = report_data["verification_results"].get("results", [])
        requirements = report_data["requirements_traceability"]["requirement_details"]
        test_matrix = report_data["test_matrix"]["matrix"]
        recommendations = report_data["recommendations"]
        
        # Generate table rows
        verification_rows = []
        for result in verification_results:
            status_class = "status-pass" if result["success"] else "status-fail"
            status_text = "✅ Passed" if result["success"] else "❌ Failed"
            details = result.get("details", {})
            detail_text = f"{details.get('states_generated', details.get('tests_run', 0))} {'states' if 'states_generated' in details else 'tests'}"
            
            verification_rows.append(f"""
                <tr>
                    <td>{result['name']}</td>
                    <td>{result['type']}</td>
                    <td class="{status_class}">{status_text}</td>
                    <td>{result['duration']:.2f}s</td>
                    <td>{detail_text}</td>
                </tr>
            """)
        
        requirements_rows = []
        for req_id, req_info in requirements.items():
            status_class = "status-pass" if req_info["fully_covered"] else "status-partial" if req_info["covered_methods"] else "status-fail"
            
            requirements_rows.append(f"""
                <tr>
                    <td>{req_id}</td>
                    <td>{req_info['description']}</td>
                    <td class="{status_class}">{req_info['status']}</td>
                    <td>{req_info['coverage_percentage']:.1f}%</td>
                </tr>
            """)
        
        test_matrix_rows = []
        for aspect, v_types in test_matrix.items():
            tla_status = v_types["TLA+ Model Checking"]["status"]
            rust_status = v_types["Rust Property Tests"]["status"]
            
            test_matrix_rows.append(f"""
                <tr>
                    <td>{aspect}</td>
                    <td>{tla_status}</td>
                    <td>{rust_status}</td>
                </tr>
            """)
        
        recommendations_list = "".join([f"<li>{rec}</li>" for rec in recommendations])
        
        # Determine overall status class
        score = overall_metrics["composite_score"]
        overall_status_class = "success" if score >= 90 else "warning" if score >= 80 else "error"
        
        # Fill template
        html_content = html_template.format(
            timestamp=report_data["metadata"]["generated_at"],
            composite_score=overall_metrics["composite_score"],
            grade=overall_metrics["grade"],
            status=overall_metrics["status"],
            overall_status_class=overall_status_class,
            verification_success_rate=overall_metrics["verification_success_rate"],
            successful_verifications=overall_metrics["successful_verifications"],
            total_verifications=overall_metrics["total_verifications"],
            requirements_coverage_percentage=overall_metrics["requirements_coverage_percentage"],
            tla_coverage_percentage=overall_metrics["tla_coverage_percentage"],
            verification_results_rows="".join(verification_rows),
            requirements_rows="".join(requirements_rows),
            test_matrix_rows="".join(test_matrix_rows),
            recommendations_list=recommendations_list
        )
        
        with open(output_path, 'w') as f:
            f.write(html_content)
        
        print(f"HTML report generated: {output_path}")

def main():
    parser = argparse.ArgumentParser(description="Generate comprehensive verification report")
    parser.add_argument("verification_report", help="Path to verification JSON report")
    parser.add_argument("--output", default="verification_coverage_report.json",
                       help="Output path for comprehensive report")
    parser.add_argument("--html", help="Generate HTML report at specified path")
    parser.add_argument("--project-root", default=".",
                       help="Project root directory")
    
    args = parser.parse_args()
    
    generator = VerificationReportGenerator(args.project_root)
    
    # Generate comprehensive JSON report
    report = generator.generate_comprehensive_report(args.verification_report, args.output)
    
    # Print summary
    overall_metrics = report["overall_metrics"]
    print(f"\n📊 Verification Report Summary")
    print(f"Overall Score: {overall_metrics['composite_score']:.1f}% ({overall_metrics['grade']})")
    print(f"Status: {overall_metrics['status']}")
    print(f"Verification Success Rate: {overall_metrics['verification_success_rate']:.1f}%")
    print(f"Requirements Coverage: {overall_metrics['requirements_coverage_percentage']:.1f}%")
    print(f"TLA+ Coverage: {overall_metrics['tla_coverage_percentage']:.1f}%")
    
    # Generate HTML report if requested
    if args.html:
        generator.generate_html_report(args.output, args.html)
    
    # Exit with appropriate code
    if overall_metrics["composite_score"] < 80:
        print(f"\n⚠️ Verification score below 80%. Review recommendations.")
        sys.exit(1)
    else:
        print(f"\n✅ Verification passed with score {overall_metrics['composite_score']:.1f}%")

if __name__ == "__main__":
    main()