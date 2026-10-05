#!/usr/bin/env python3
"""Print the HDL files listed in files.qip, one per line, for Verilator's -f option.

The Quartus file list is the single list of what the core is built from; the
core-level simulation reads the same list so the two cannot drift apart.
"""
import os
import sys

root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
for line in open(os.path.join(root, 'files.qip')):
    parts = line.split()
    if len(parts) >= 4 and parts[2] in ('VERILOG_FILE', 'SYSTEMVERILOG_FILE'):
        print(parts[3])
