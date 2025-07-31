#!/usr/bin/env python3
"""
State Space Optimization Script for Nyx Protocol Model Checking
Provides advanced optimization techniques for efficient state space exploration
"""

import os
import sys
import json
import argparse
from typing import Dict, List, Tuple
from run_model_checking import TLCRunner, CounterexampleAnalyzer

class StateSpaceOptimizer:
    """Advanced state space optimization for TLA+ model checking"""
    
    def __init__(self):
        self.optimization_strategies = {
            "symmetry_reduction": self._apply_symmetry_reduction,
            "bounded_checking": self._apply_bounded_checking,
            "partial_order_reduction": self._apply_partial_order_reduction,
            "state_space_reduction": self._apply_state_space_reduction
        }
    
    def _apply_symmetry_reduction(self, config_content: str) -> str:
        """Apply symmetry reduction optimizations"""
        if "SYMMETRY" not in config_content:
            # Add symmetry if not present
            lines = config_content.split('\n')
            constants_line = -1
            for i, line in enumerate(lines):
                if line.startswith("CONSTANTS"):
                    constants_line = i
                    break
            
            if constants_line >= 0:
                lines.insert(constants_line + 1, "SYMMETRY NodeSymmetry")
                return '\n'.join(lines)
        
        return config_content
    
    def _apply_bounded_checking(self, config_content: str) -> str:
        """Apply bounded model checking optimizations"""
        # Add depth bounds for large state spaces
        if "CONSTANTS" in config_content and "NodeCount = " in config_content:
            # Extract NodeCount value
            for line in config_content.split('\n'):
                if "NodeCount = " in line:
                    try:
                        node_count = int(line.split('=')[1].strip())
                        if node_count > 15:
                            # Add bounded checking for large node counts
                            config_content += "\nCONSTRAINT operation_timer <= 50\n"
                            config_content += "CONSTRAINT retry_count <= 2\n"
                            config_content += "CONSTRAINT Cardinality(active_streams) <= 3\n"
                    except (ValueError, IndexError):
                        pass
        
        return config_content
    
    def _apply_partial_order_reduction(self, config_content: str) -> str:
        """Apply partial order reduction techniques"""
        # This would require TLA+ model modifications for independence
        # For now, we add a comment indicating where POR could be applied
        if "SPECIFICATION" in config_content:
            config_content += "\n\n\\ Partial Order Reduction could be applied to:\n"
            config_content += "\\ - Independent timer advancement actions\n"
            config_content += "\\ - Concurrent stream operations\n"
            config_content += "\\ - Parallel capability negotiations\n"
        
        return config_content
    
    def _apply_state_space_reduction(self, config_content: str) -> str:
        """Apply general state space reduction techniques"""
        # Add state constraints to reduce exploration space
        constraints = [
            "CONSTRAINT Len(path) <= 5",  # Limit path length
            "CONSTRAINT key_rotation_timer <= 200",  # Limit timer values
            "CONSTRAINT timeout_count <= 5"  # Limit timeout occurrences
        ]
        
        for constraint in constraints:
            if constraint not in config_content:
                config_content += f"\n{constraint}"
        
        return config_content
    
    def optimize_configuration(self, config_file: str, strategies: List[str] = None) -> str:
        """Optimize a configuration file using specified strategies"""
        if not os.path.exists(config_file):
            raise FileNotFoundError(f"Configuration file not found: {config_file}")
        
        with open(config_file, 'r') as f:
            content = f.read()
        
        if strategies is None:
            strategies = list(self.optimization_strategies.keys())
        
        optimized_content = content
        for strategy in strategies:
            if strategy in self.optimization_strategies:
                optimized_content = self.optimization_strategies[strategy](optimized_content)
        
        # Create optimized configuration file
        optimized_file = config_file.replace('.cfg', '_optimized.cfg')
        with open(optimized_file, 'w') as f:
            f.write(optimized_content)
        
        return optimized_file
    
    def analyze_optimization_impact(self, original_result, optimized_result) -> Dict:
        """Analyze the impact of optimizations on model checking performance"""
        analysis = {
            "performance_improvement": {
                "time_reduction": 0.0,
                "state_reduction": 0.0,
                "memory_efficiency": 0.0
            },
            "coverage_impact": {
                "coverage_change": 0.0,
                "completeness_preserved": True
            },
            "recommendations": []
        }
        
        # Calculate performance improvements
        if original_result.duration > 0:
            time_improvement = (original_result.duration - optimized_result.duration) / original_result.duration
            analysis["performance_improvement"]["time_reduction"] = time_improvement * 100
        
        if original_result.states_generated > 0:
            state_reduction = (original_result.states_generated - optimized_result.states_generated) / original_result.states_generated
            analysis["performance_improvement"]["state_reduction"] = state_reduction * 100
        
        # Generate recommendations based on results
        if analysis["performance_improvement"]["time_reduction"] > 20:
            analysis["recommendations"].append("Significant time improvement achieved - consider using optimized configuration")
        
        if analysis["performance_improvement"]["state_reduction"] > 30:
            analysis["recommendations"].append("Major state space reduction - verify completeness is maintained")
        
        if optimized_result.success and not original_result.success:
            analysis["recommendations"].append("Optimization enabled successful completion - recommended for production use")
        
        return analysis

class BoundedModelChecker:
    """Specialized bounded model checking for large state spaces"""
    
    def __init__(self, max_depth: int = 50, max_states: int = 1000000):
        self.max_depth = max_depth
        self.max_states = max_states
    
    def create_bounded_config(self, base_config: str, bounds: Dict[str, int]) -> str:
        """Create a bounded version of a configuration"""
        with open(base_config, 'r') as f:
            content = f.read()
        
        # Add bounded constraints
        bounded_content = content + "\n\n\\ Bounded Model Checking Constraints\n"
        
        for constraint_name, bound_value in bounds.items():
            bounded_content += f"CONSTRAINT {constraint_name} <= {bound_value}\n"
        
        # Add depth and state limits
        bounded_content += f"CONSTRAINT operation_timer <= {self.max_depth}\n"
        
        bounded_file = base_config.replace('.cfg', '_bounded.cfg')
        with open(bounded_file, 'w') as f:
            f.write(bounded_content)
        
        return bounded_file
    
    def run_incremental_bounded_checking(self, config_file: str, runner: TLCRunner) -> List:
        """Run incremental bounded model checking with increasing bounds"""
        results = []
        
        # Define incremental bounds
        depth_bounds = [10, 25, 50, 100]
        
        for depth in depth_bounds:
            print(f"Running bounded checking with depth {depth}...")
            
            bounds = {
                "operation_timer": depth,
                "retry_count": min(3, depth // 10),
                "timeout_count": min(5, depth // 5)
            }
            
            bounded_config = self.create_bounded_config(config_file, bounds)
            result = runner.run_tlc(bounded_config, timeout=300)
            results.append((depth, result))
            
            # Stop if we find a counterexample
            if not result.success and result.counterexample:
                print(f"Counterexample found at depth {depth}")
                break
            
            # Stop if we complete successfully
            if result.success:
                print(f"Completed successfully at depth {depth}")
                break
        
        return results

def main():
    parser = argparse.ArgumentParser(description="Optimize TLA+ model checking for Nyx protocol")
    parser.add_argument("--config", required=True, help="Configuration file to optimize")
    parser.add_argument("--strategies", nargs='+', 
                       choices=["symmetry_reduction", "bounded_checking", 
                               "partial_order_reduction", "state_space_reduction"],
                       default=["symmetry_reduction", "bounded_checking", "state_space_reduction"],
                       help="Optimization strategies to apply")
    parser.add_argument("--compare", action="store_true", 
                       help="Compare original vs optimized performance")
    parser.add_argument("--bounded", action="store_true",
                       help="Run incremental bounded model checking")
    parser.add_argument("--output", default="optimization_report.json",
                       help="Output file for optimization report")
    
    args = parser.parse_args()
    
    optimizer = StateSpaceOptimizer()
    runner = TLCRunner()
    
    print("🔧 OPTIMIZING MODEL CHECKING CONFIGURATION")
    print("=" * 60)
    
    # Optimize configuration
    optimized_config = optimizer.optimize_configuration(args.config, args.strategies)
    print(f"Created optimized configuration: {optimized_config}")
    
    if args.compare:
        print("\n📊 COMPARING PERFORMANCE")
        print("=" * 40)
        
        # Run original configuration
        print("Running original configuration...")
        original_result = runner.run_tlc(args.config, timeout=300)
        
        # Run optimized configuration
        print("Running optimized configuration...")
        optimized_result = runner.run_tlc(optimized_config, timeout=300)
        
        # Analyze optimization impact
        impact_analysis = optimizer.analyze_optimization_impact(original_result, optimized_result)
        
        print(f"\nOptimization Results:")
        print(f"Time reduction: {impact_analysis['performance_improvement']['time_reduction']:.1f}%")
        print(f"State reduction: {impact_analysis['performance_improvement']['state_reduction']:.1f}%")
        
        for recommendation in impact_analysis['recommendations']:
            print(f"💡 {recommendation}")
        
        # Save detailed report
        report = {
            "original_result": {
                "success": original_result.success,
                "duration": original_result.duration,
                "states_generated": original_result.states_generated
            },
            "optimized_result": {
                "success": optimized_result.success,
                "duration": optimized_result.duration,
                "states_generated": optimized_result.states_generated
            },
            "impact_analysis": impact_analysis
        }
        
        with open(args.output, 'w') as f:
            json.dump(report, f, indent=2)
        
        print(f"\nDetailed report saved to: {args.output}")
    
    if args.bounded:
        print("\n🎯 INCREMENTAL BOUNDED MODEL CHECKING")
        print("=" * 50)
        
        bounded_checker = BoundedModelChecker()
        bounded_results = bounded_checker.run_incremental_bounded_checking(args.config, runner)
        
        print("\nBounded checking results:")
        for depth, result in bounded_results:
            status = "✓ SUCCESS" if result.success else "✗ FAILED"
            print(f"Depth {depth:3d}: {status} ({result.duration:.1f}s, {result.states_generated} states)")

if __name__ == "__main__":
    main()