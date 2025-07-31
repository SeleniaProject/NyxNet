#!/usr/bin/env python3
"""
Comprehensive Verification Coverage Report Generator for Nyx Protocol
Generates detailed coverage reports and recommendations for formal verification
"""

import os
import sys
import json
import argparse
import subprocess
from datetime import datetime
from typing import Dict, List, Tuple, Optional
from pathlib import Path

class VerificationCoverageReporter:
    """Generate comprehensive verification coverage reports"""
    
    def __init__(self, formal_dir: str = "formal"):
        self.formal_dir = Path(formal_dir)
        self.tla_file = self.formal_dir / "nyx_multipath_plugin.tla"
        self.coverage_data = {}
        self.verification_results = {}
        
    def analyze_model_coverage(self) -> Dict:
        """Analyze coverage of the TLA+ model itself"""
        if not self.tla_file.exists():
            return {"error": "TLA+ model file not found"}
        
        with open(self.tla_file, 'r') as f:
            content = f.read()
        
        coverage = {
            "model_structure": self._analyze_model_structure(content),
            "invariant_coverage": self._analyze_invariant_coverage(content),
            "action_coverage": self._analyze_action_coverage(content),
            "proof_coverage": self._analyze_proof_coverage(content),
            "property_coverage": self._analyze_property_coverage(content)
        }
        
        return coverage
    
    def _analyze_model_structure(self, content: str) -> Dict:
        """Analyze the structure and completeness of the TLA+ model"""
        structure = {
            "constants_defined": 0,
            "variables_defined": 0,
            "actions_defined": 0,
            "invariants_defined": 0,
            "properties_defined": 0,
            "theorems_defined": 0,
            "completeness_score": 0.0
        }
        
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            
            if line.startswith("CONSTANTS"):
                # Count constants in the line and following lines
                const_line = line
                i = lines.index(line) + 1
                while i < len(lines) and not lines[i].strip().startswith(("VARIABLES", "ASSUME", "THEOREM")):
                    if lines[i].strip():
                        const_line += " " + lines[i].strip()
                    i += 1
                
                # Count comma-separated constants
                const_part = const_line.replace("CONSTANTS", "").strip()
                if const_part:
                    structure["constants_defined"] = len([c.strip() for c in const_part.split(',') if c.strip()])
            
            elif line.startswith("VARIABLES"):
                # Count variables
                var_section = []
                i = lines.index(line)
                while i < len(lines) and not lines[i].strip().startswith(("Init", "Next", "Spec")):
                    if lines[i].strip() and not lines[i].strip().startswith("(*"):
                        var_section.append(lines[i])
                    i += 1
                
                # Count variable definitions
                var_text = ' '.join(var_section).replace("VARIABLES", "")
                vars_list = [v.strip() for v in var_text.split(',') if v.strip() and not v.strip().startswith('\\*')]
                structure["variables_defined"] = len([v for v in vars_list if v and not v.startswith('(*')])
            
            elif " == " in line and not line.startswith("(*") and not line.startswith("\\*"):
                # Count action definitions
                if any(keyword in line for keyword in ["UNCHANGED", "/\\", "\\/"]):
                    structure["actions_defined"] += 1
            
            elif line.startswith("Inv_") and " == " in line:
                structure["invariants_defined"] += 1
            
            elif any(prop in line for prop in ["<>", "[]", "~>"]) and " == " in line:
                structure["properties_defined"] += 1
            
            elif line.startswith("THEOREM"):
                structure["theorems_defined"] += 1
        
        # Calculate completeness score
        expected_minimums = {
            "constants_defined": 5,
            "variables_defined": 10,
            "actions_defined": 15,
            "invariants_defined": 8,
            "properties_defined": 5,
            "theorems_defined": 10
        }
        
        scores = []
        for metric, expected in expected_minimums.items():
            actual = structure[metric]
            score = min(1.0, actual / expected) if expected > 0 else 1.0
            scores.append(score)
        
        structure["completeness_score"] = sum(scores) / len(scores)
        
        return structure
    
    def _analyze_invariant_coverage(self, content: str) -> Dict:
        """Analyze coverage of invariants"""
        invariants = []
        theorems = []
        
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            
            if line.startswith("Inv_") and " == " in line:
                inv_name = line.split(" == ")[0].strip()
                invariants.append(inv_name)
            
            elif line.startswith("THEOREM") and "Inv_" in line:
                theorem_name = line.split("THEOREM")[1].split("==")[0].strip()
                theorems.append(theorem_name)
        
        # Check which invariants have corresponding theorems
        proven_invariants = []
        for inv in invariants:
            for theorem in theorems:
                if inv in theorem:
                    proven_invariants.append(inv)
                    break
        
        coverage = {
            "total_invariants": len(invariants),
            "proven_invariants": len(proven_invariants),
            "unproven_invariants": list(set(invariants) - set(proven_invariants)),
            "coverage_percentage": (len(proven_invariants) / len(invariants) * 100) if invariants else 0,
            "invariant_list": invariants,
            "theorem_list": theorems
        }
        
        return coverage
    
    def _analyze_action_coverage(self, content: str) -> Dict:
        """Analyze coverage of actions in Next relation"""
        actions = []
        next_actions = []
        
        lines = content.split('\n')
        in_next_definition = False
        
        for line in lines:
            line = line.strip()
            
            # Find action definitions
            if (" == " in line and 
                not line.startswith("(*") and 
                not line.startswith("\\*") and
                not line.startswith("Inv_") and
                not line.startswith("THEOREM") and
                line[0].isupper()):
                
                action_name = line.split(" == ")[0].strip()
                if action_name not in ["Init", "Next", "Spec", "TypeInvariant"]:
                    actions.append(action_name)
            
            # Find actions in Next relation
            if line.startswith("Next =="):
                in_next_definition = True
                continue
            
            if in_next_definition:
                if line.startswith("\\/ "):
                    action_name = line.replace("\\/ ", "").strip()
                    next_actions.append(action_name)
                elif line and not line.startswith("\\") and not line.startswith("(*"):
                    in_next_definition = False
        
        # Check coverage
        covered_actions = [action for action in actions if action in next_actions]
        uncovered_actions = [action for action in actions if action not in next_actions]
        
        coverage = {
            "total_actions": len(actions),
            "covered_actions": len(covered_actions),
            "uncovered_actions": uncovered_actions,
            "coverage_percentage": (len(covered_actions) / len(actions) * 100) if actions else 0,
            "action_list": actions,
            "next_actions": next_actions
        }
        
        return coverage
    
    def _analyze_proof_coverage(self, content: str) -> Dict:
        """Analyze coverage of formal proofs"""
        theorems = []
        proofs = []
        
        lines = content.split('\n')
        current_theorem = None
        in_proof = False
        proof_depth = 0
        
        for line in lines:
            line = line.strip()
            
            if line.startswith("THEOREM"):
                theorem_name = line.split("THEOREM")[1].split("==")[0].strip()
                theorems.append(theorem_name)
                current_theorem = theorem_name
                in_proof = False
            
            elif current_theorem and line.startswith("<"):
                # Start of proof
                in_proof = True
                proof_depth = line.count("<")
            
            elif in_proof and line.startswith("QED"):
                # End of proof
                proofs.append(current_theorem)
                current_theorem = None
                in_proof = False
        
        coverage = {
            "total_theorems": len(theorems),
            "proven_theorems": len(proofs),
            "unproven_theorems": list(set(theorems) - set(proofs)),
            "coverage_percentage": (len(proofs) / len(theorems) * 100) if theorems else 0,
            "theorem_list": theorems,
            "proof_list": proofs
        }
        
        return coverage
    
    def _analyze_property_coverage(self, content: str) -> Dict:
        """Analyze coverage of temporal properties"""
        safety_properties = []
        liveness_properties = []
        
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            
            if " == " in line and not line.startswith("(*"):
                prop_def = line.split(" == ")[1] if " == " in line else ""
                
                # Safety properties (invariants)
                if "[]" in prop_def and not "<>" in prop_def:
                    prop_name = line.split(" == ")[0].strip()
                    safety_properties.append(prop_name)
                
                # Liveness properties
                elif "<>" in prop_def or "~>" in prop_def:
                    prop_name = line.split(" == ")[0].strip()
                    liveness_properties.append(prop_name)
        
        coverage = {
            "safety_properties": len(safety_properties),
            "liveness_properties": len(liveness_properties),
            "total_properties": len(safety_properties) + len(liveness_properties),
            "safety_list": safety_properties,
            "liveness_list": liveness_properties,
            "balance_score": self._calculate_property_balance(safety_properties, liveness_properties)
        }
        
        return coverage
    
    def _calculate_property_balance(self, safety: List, liveness: List) -> float:
        """Calculate balance between safety and liveness properties"""
        total = len(safety) + len(liveness)
        if total == 0:
            return 0.0
        
        # Ideal ratio is roughly 60% safety, 40% liveness
        safety_ratio = len(safety) / total
        ideal_safety_ratio = 0.6
        
        # Calculate how close we are to ideal balance
        balance_score = 1.0 - abs(safety_ratio - ideal_safety_ratio)
        return max(0.0, balance_score)
    
    def analyze_configuration_coverage(self) -> Dict:
        """Analyze coverage across different model checking configurations"""
        config_files = list(self.formal_dir.glob("*.cfg"))
        
        coverage = {
            "total_configurations": len(config_files),
            "configuration_analysis": {},
            "coverage_matrix": {},
            "recommendations": []
        }
        
        for config_file in config_files:
            config_name = config_file.stem
            config_analysis = self._analyze_single_configuration(config_file)
            coverage["configuration_analysis"][config_name] = config_analysis
        
        # Generate coverage matrix
        coverage["coverage_matrix"] = self._generate_coverage_matrix(coverage["configuration_analysis"])
        
        # Generate recommendations
        coverage["recommendations"] = self._generate_coverage_recommendations(coverage)
        
        return coverage
    
    def _analyze_single_configuration(self, config_file: Path) -> Dict:
        """Analyze a single configuration file"""
        with open(config_file, 'r') as f:
            content = f.read()
        
        analysis = {
            "specification": None,
            "invariants": [],
            "properties": [],
            "constants": {},
            "symmetry": False,
            "constraints": [],
            "complexity_score": 0.0
        }
        
        lines = content.split('\n')
        
        for line in lines:
            line = line.strip()
            
            if line.startswith("SPECIFICATION"):
                analysis["specification"] = line.split("SPECIFICATION")[1].strip()
            
            elif line.startswith("INVARIANTS"):
                inv_line = line.replace("INVARIANTS", "").strip()
                analysis["invariants"] = [inv.strip() for inv in inv_line.split() if inv.strip()]
            
            elif line.startswith("PROPERTIES"):
                prop_line = line.replace("PROPERTIES", "").strip()
                analysis["properties"] = [prop.strip() for prop in prop_line.split() if prop.strip()]
            
            elif line.startswith("CONSTANTS"):
                const_line = line.replace("CONSTANTS", "").strip()
                # Parse constants like "NodeCount = 10"
                for const in const_line.split():
                    if "=" in const:
                        key, value = const.split("=", 1)
                        try:
                            analysis["constants"][key.strip()] = int(value.strip())
                        except ValueError:
                            analysis["constants"][key.strip()] = value.strip()
            
            elif line.startswith("SYMMETRY"):
                analysis["symmetry"] = True
            
            elif line.startswith("CONSTRAINT"):
                analysis["constraints"].append(line.replace("CONSTRAINT", "").strip())
        
        # Calculate complexity score
        complexity_factors = [
            analysis["constants"].get("NodeCount", 1),
            len(analysis["constants"].get("CapSet", [])) if isinstance(analysis["constants"].get("CapSet"), list) else analysis["constants"].get("CapSet", 1),
            analysis["constants"].get("MaxStreams", 1),
            analysis["constants"].get("MaxRetries", 1)
        ]
        
        analysis["complexity_score"] = sum(complexity_factors) / len(complexity_factors)
        
        return analysis
    
    def _generate_coverage_matrix(self, config_analysis: Dict) -> Dict:
        """Generate a coverage matrix showing which invariants/properties are tested by which configs"""
        all_invariants = set()
        all_properties = set()
        
        # Collect all invariants and properties
        for config_name, analysis in config_analysis.items():
            all_invariants.update(analysis["invariants"])
            all_properties.update(analysis["properties"])
        
        matrix = {
            "invariants": {},
            "properties": {}
        }
        
        # Build invariant coverage matrix
        for invariant in all_invariants:
            matrix["invariants"][invariant] = []
            for config_name, analysis in config_analysis.items():
                if invariant in analysis["invariants"]:
                    matrix["invariants"][invariant].append(config_name)
        
        # Build property coverage matrix
        for prop in all_properties:
            matrix["properties"][prop] = []
            for config_name, analysis in config_analysis.items():
                if prop in analysis["properties"]:
                    matrix["properties"][prop].append(config_name)
        
        return matrix
    
    def _generate_coverage_recommendations(self, coverage_data: Dict) -> List[Dict]:
        """Generate recommendations for improving coverage"""
        recommendations = []
        
        config_analysis = coverage_data["configuration_analysis"]
        coverage_matrix = coverage_data["coverage_matrix"]
        
        # Check for untested invariants
        untested_invariants = [inv for inv, configs in coverage_matrix["invariants"].items() if not configs]
        if untested_invariants:
            recommendations.append({
                "priority": "high",
                "category": "invariant_coverage",
                "description": f"Add testing for {len(untested_invariants)} untested invariants",
                "details": untested_invariants[:5],  # Show first 5
                "action": "Create configurations that test these invariants"
            })
        
        # Check for untested properties
        untested_properties = [prop for prop, configs in coverage_matrix["properties"].items() if not configs]
        if untested_properties:
            recommendations.append({
                "priority": "high",
                "category": "property_coverage",
                "description": f"Add testing for {len(untested_properties)} untested properties",
                "details": untested_properties[:5],
                "action": "Create configurations that test these properties"
            })
        
        # Check for low complexity configurations
        low_complexity_configs = [
            name for name, analysis in config_analysis.items() 
            if analysis["complexity_score"] < 5
        ]
        if len(low_complexity_configs) > len(config_analysis) * 0.7:
            recommendations.append({
                "priority": "medium",
                "category": "complexity_coverage",
                "description": "Most configurations have low complexity",
                "details": low_complexity_configs,
                "action": "Add high-complexity configurations to test scalability"
            })
        
        # Check for missing symmetry optimization
        no_symmetry_configs = [
            name for name, analysis in config_analysis.items() 
            if not analysis["symmetry"] and analysis["complexity_score"] > 10
        ]
        if no_symmetry_configs:
            recommendations.append({
                "priority": "low",
                "category": "optimization",
                "description": "High-complexity configurations without symmetry reduction",
                "details": no_symmetry_configs,
                "action": "Add SYMMETRY declarations to improve performance"
            })
        
        return recommendations
    
    def generate_comprehensive_report(self) -> Dict:
        """Generate a comprehensive verification coverage report"""
        report = {
            "metadata": {
                "generation_timestamp": datetime.now().isoformat(),
                "formal_directory": str(self.formal_dir),
                "tla_model": str(self.tla_file),
                "report_version": "1.0"
            },
            "model_coverage": self.analyze_model_coverage(),
            "configuration_coverage": self.analyze_configuration_coverage(),
            "overall_assessment": {},
            "improvement_plan": []
        }
        
        # Calculate overall assessment
        model_cov = report["model_coverage"]
        config_cov = report["configuration_coverage"]
        
        overall_scores = []
        
        if "model_structure" in model_cov:
            overall_scores.append(model_cov["model_structure"]["completeness_score"])
        
        if "invariant_coverage" in model_cov:
            overall_scores.append(model_cov["invariant_coverage"]["coverage_percentage"] / 100)
        
        if "action_coverage" in model_cov:
            overall_scores.append(model_cov["action_coverage"]["coverage_percentage"] / 100)
        
        if "proof_coverage" in model_cov:
            overall_scores.append(model_cov["proof_coverage"]["coverage_percentage"] / 100)
        
        report["overall_assessment"] = {
            "overall_score": sum(overall_scores) / len(overall_scores) if overall_scores else 0,
            "strengths": self._identify_strengths(report),
            "weaknesses": self._identify_weaknesses(report),
            "readiness_level": self._assess_readiness_level(report)
        }
        
        # Generate improvement plan
        report["improvement_plan"] = self._generate_improvement_plan(report)
        
        return report
    
    def _identify_strengths(self, report: Dict) -> List[str]:
        """Identify strengths in the verification coverage"""
        strengths = []
        
        model_cov = report["model_coverage"]
        
        if model_cov.get("model_structure", {}).get("completeness_score", 0) > 0.8:
            strengths.append("Comprehensive model structure with good coverage of protocol elements")
        
        if model_cov.get("invariant_coverage", {}).get("coverage_percentage", 0) > 80:
            strengths.append("High invariant coverage with formal proofs")
        
        if model_cov.get("proof_coverage", {}).get("coverage_percentage", 0) > 70:
            strengths.append("Strong formal proof coverage for safety properties")
        
        config_count = report["configuration_coverage"]["total_configurations"]
        if config_count >= 6:
            strengths.append(f"Good variety of test configurations ({config_count} configurations)")
        
        return strengths
    
    def _identify_weaknesses(self, report: Dict) -> List[str]:
        """Identify weaknesses in the verification coverage"""
        weaknesses = []
        
        model_cov = report["model_coverage"]
        
        if model_cov.get("action_coverage", {}).get("coverage_percentage", 0) < 70:
            weaknesses.append("Low action coverage in Next relation")
        
        if model_cov.get("property_coverage", {}).get("liveness_properties", 0) < 3:
            weaknesses.append("Insufficient liveness property coverage")
        
        unproven_invariants = model_cov.get("invariant_coverage", {}).get("unproven_invariants", [])
        if len(unproven_invariants) > 2:
            weaknesses.append(f"Multiple unproven invariants ({len(unproven_invariants)})")
        
        config_recs = report["configuration_coverage"]["recommendations"]
        high_priority_recs = [r for r in config_recs if r["priority"] == "high"]
        if len(high_priority_recs) > 2:
            weaknesses.append("Multiple high-priority configuration coverage gaps")
        
        return weaknesses
    
    def _assess_readiness_level(self, report: Dict) -> str:
        """Assess the overall readiness level of the verification"""
        overall_score = report["overall_assessment"]["overall_score"]
        
        if overall_score >= 0.9:
            return "production_ready"
        elif overall_score >= 0.8:
            return "near_production"
        elif overall_score >= 0.7:
            return "development_complete"
        elif overall_score >= 0.5:
            return "development_active"
        else:
            return "development_early"
    
    def _generate_improvement_plan(self, report: Dict) -> List[Dict]:
        """Generate a prioritized improvement plan"""
        plan = []
        
        weaknesses = report["overall_assessment"]["weaknesses"]
        config_recs = report["configuration_coverage"]["recommendations"]
        
        # Convert weaknesses to action items
        for weakness in weaknesses:
            if "action coverage" in weakness:
                plan.append({
                    "priority": 1,
                    "category": "model_completeness",
                    "task": "Review and add missing actions to Next relation",
                    "estimated_effort": "medium",
                    "impact": "high"
                })
            
            elif "liveness property" in weakness:
                plan.append({
                    "priority": 2,
                    "category": "property_coverage",
                    "task": "Add comprehensive liveness properties for progress guarantees",
                    "estimated_effort": "high",
                    "impact": "high"
                })
            
            elif "unproven invariants" in weakness:
                plan.append({
                    "priority": 1,
                    "category": "proof_completeness",
                    "task": "Complete formal proofs for all invariants",
                    "estimated_effort": "high",
                    "impact": "critical"
                })
        
        # Add configuration recommendations
        for rec in config_recs:
            if rec["priority"] == "high":
                plan.append({
                    "priority": 1,
                    "category": "configuration_coverage",
                    "task": rec["description"],
                    "estimated_effort": "low",
                    "impact": "medium"
                })
        
        # Sort by priority
        plan.sort(key=lambda x: x["priority"])
        
        return plan
    
    def create_ci_integration_script(self) -> str:
        """Create a CI integration script for continuous verification"""
        script = '''#!/bin/bash
# Continuous Integration Script for Nyx Protocol Formal Verification
# This script runs comprehensive model checking and generates coverage reports

set -e

echo "🔍 Starting Nyx Protocol Formal Verification CI"
echo "================================================"

# Configuration
FORMAL_DIR="formal"
REPORT_DIR="verification_reports"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# Create report directory
mkdir -p "$REPORT_DIR"

# Run model checking
echo "📋 Running model checking..."
python3 "$FORMAL_DIR/run_model_checking.py" \\
    --timeout 600 \\
    --output "$REPORT_DIR/model_checking_$TIMESTAMP.json"

# Generate coverage report
echo "📊 Generating coverage report..."
python3 "$FORMAL_DIR/generate-verification-report.py" \\
    --output "$REPORT_DIR/coverage_report_$TIMESTAMP.json"

# Analyze any counterexamples
if ls "$REPORT_DIR"/*counterexample* 1> /dev/null 2>&1; then
    echo "🔍 Analyzing counterexamples..."
    for ce_file in "$REPORT_DIR"/*counterexample*; do
        python3 "$FORMAL_DIR/counterexample_analyzer.py" \\
            --input "$ce_file" \\
            --output "${ce_file%.txt}_analysis.json" \\
            --visualize \\
            --generate-test
    done
fi

# Check verification quality
echo "✅ Checking verification quality..."
COVERAGE_SCORE=$(python3 -c "
import json
with open('$REPORT_DIR/coverage_report_$TIMESTAMP.json') as f:
    data = json.load(f)
print(data['overall_assessment']['overall_score'])
")

echo "Coverage Score: $COVERAGE_SCORE"

# Set CI status based on coverage
if (( $(echo "$COVERAGE_SCORE > 0.8" | bc -l) )); then
    echo "✅ Verification quality: EXCELLENT"
    exit 0
elif (( $(echo "$COVERAGE_SCORE > 0.7" | bc -l) )); then
    echo "⚠️  Verification quality: GOOD (room for improvement)"
    exit 0
elif (( $(echo "$COVERAGE_SCORE > 0.5" | bc -l) )); then
    echo "⚠️  Verification quality: FAIR (needs improvement)"
    exit 1
else
    echo "❌ Verification quality: POOR (requires attention)"
    exit 1
fi
'''
        return script

def main():
    parser = argparse.ArgumentParser(description="Generate verification coverage report for Nyx protocol")
    parser.add_argument("--formal-dir", default="formal", help="Formal verification directory")
    parser.add_argument("--output", default="verification_coverage_report.json", 
                       help="Output file for coverage report")
    parser.add_argument("--ci-script", action="store_true",
                       help="Generate CI integration script")
    parser.add_argument("--verbose", action="store_true", help="Verbose output")
    
    args = parser.parse_args()
    
    reporter = VerificationCoverageReporter(args.formal_dir)
    
    print("📊 GENERATING VERIFICATION COVERAGE REPORT")
    print("=" * 60)
    
    try:
        # Generate comprehensive report
        report = reporter.generate_comprehensive_report()
        
        # Save report
        with open(args.output, 'w') as f:
            json.dump(report, f, indent=2)
        
        print(f"Coverage report saved to: {args.output}")
        
        # Print summary
        overall = report["overall_assessment"]
        print(f"\n📋 VERIFICATION SUMMARY")
        print("=" * 40)
        print(f"Overall Score: {overall['overall_score']:.2f}")
        print(f"Readiness Level: {overall['readiness_level']}")
        
        if overall["strengths"]:
            print(f"\n✅ Strengths:")
            for strength in overall["strengths"]:
                print(f"  • {strength}")
        
        if overall["weaknesses"]:
            print(f"\n⚠️  Areas for Improvement:")
            for weakness in overall["weaknesses"]:
                print(f"  • {weakness}")
        
        # Show improvement plan
        improvement_plan = report["improvement_plan"]
        if improvement_plan:
            print(f"\n🔧 IMPROVEMENT PLAN")
            print("=" * 30)
            for i, item in enumerate(improvement_plan[:5], 1):  # Show top 5
                print(f"{i}. {item['task']} (Priority: {item['priority']}, Impact: {item['impact']})")
        
        # Generate CI script if requested
        if args.ci_script:
            ci_script = reporter.create_ci_integration_script()
            ci_file = "ci_verification.sh"
            with open(ci_file, 'w') as f:
                f.write(ci_script)
            
            # Make executable
            os.chmod(ci_file, 0o755)
            print(f"CI integration script saved to: {ci_file}")
        
        # Exit with appropriate code based on readiness level
        readiness = overall['readiness_level']
        if readiness in ['production_ready', 'near_production']:
            sys.exit(0)
        elif readiness in ['development_complete', 'development_active']:
            sys.exit(1)  # Warning level
        else:
            sys.exit(2)  # Error level
        
    except Exception as e:
        print(f"Error generating coverage report: {e}")
        if args.verbose:
            import traceback
            traceback.print_exc()
        sys.exit(1)

if __name__ == "__main__":
    main()