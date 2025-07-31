#!/usr/bin/env python3
"""
Simple test script to validate TLC configuration files
"""

import os
import subprocess
import sys

def test_configuration_syntax(config_file):
    """Test if a configuration file has valid syntax"""
    print(f"Testing configuration: {config_file}")
    
    # Check if file exists
    if not os.path.exists(config_file):
        print(f"  ❌ File not found: {config_file}")
        return False
    
    # Read and validate basic structure
    try:
        with open(config_file, 'r') as f:
            content = f.read()
        
        required_sections = ['SPECIFICATION', 'INVARIANTS', 'CONSTANTS']
        for section in required_sections:
            if section not in content:
                print(f"  ❌ Missing required section: {section}")
                return False
        
        print(f"  ✅ Configuration syntax appears valid")
        return True
        
    except Exception as e:
        print(f"  ❌ Error reading file: {e}")
        return False

def main():
    """Test all configuration files"""
    config_files = [
        "basic.cfg",
        "comprehensive.cfg", 
        "scalability.cfg",
        "capability_stress.cfg",
        "liveness_focus.cfg"
    ]
    
    print("Testing TLC configuration files...")
    print("=" * 50)
    
    all_valid = True
    for config_file in config_files:
        if not test_configuration_syntax(config_file):
            all_valid = False
        print()
    
    if all_valid:
        print("✅ All configuration files appear valid")
        return 0
    else:
        print("❌ Some configuration files have issues")
        return 1

if __name__ == "__main__":
    sys.exit(main())