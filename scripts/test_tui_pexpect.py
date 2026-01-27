#!/usr/bin/env python3
"""
TUI Integration Tests using pexpect.

Tests the interactive TUI with:
- Tool approval dialogs (approve/reject)
- Question dialogs from ask_user_question tool

Requires: python3-pexpect

Usage:
    python3 scripts/test_tui_pexpect.py
"""

import pexpect
import time
import os
import sys

# Change to project root
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(SCRIPT_DIR)
os.chdir(PROJECT_ROOT)


def run_tui_test(name, prompt, expected_pattern, action_key, timeout_secs=50):
    """Run a single TUI test.

    Args:
        name: Test name for display
        prompt: User input to send to the TUI
        expected_pattern: String to look for in output
        action_key: Key to send when pattern is found (e.g., '\r' for Enter, 'n' for reject)
        timeout_secs: How long to wait for the pattern

    Returns:
        dict with success, found_pattern, action_taken
    """
    print(f"\n{'='*60}")
    print(f"TEST: {name}")
    print(f"{'='*60}")

    child = pexpect.spawn(
        "cargo run -p cowork-cli --release",
        encoding='utf-8',
        timeout=90,
        dimensions=(40, 120)
    )

    output = []
    result = {"success": False, "found_pattern": False, "action_taken": False}

    try:
        # Wait for TUI to initialize
        for _ in range(10):
            try:
                output.append(child.read_nonblocking(8192, timeout=0.5))
            except pexpect.TIMEOUT:
                pass

        full = ''.join(output)
        if 'Welcome' not in full:
            print("[FAIL] TUI did not initialize")
            return result

        print("[OK] TUI initialized")

        # Type the prompt character by character (TUI is in raw mode)
        print(f"[SEND] {prompt[:60]}...")
        for char in prompt:
            child.send(char)
            time.sleep(0.02)
        child.send('\r')  # Enter to submit

        # Wait for expected pattern
        print(f"[WAIT] Looking for: {expected_pattern}")
        for i in range(timeout_secs * 2):
            try:
                data = child.read_nonblocking(8192, timeout=0.5)
                output.append(data)

                if expected_pattern in data:
                    result["found_pattern"] = True
                    print(f"[OK] Found '{expected_pattern}' at iteration {i}")
                    break

            except pexpect.TIMEOUT:
                pass
            except pexpect.EOF:
                break

        if result["found_pattern"]:
            time.sleep(1)
            print(f"[ACTION] Sending: {repr(action_key)}")
            child.send(action_key)
            result["action_taken"] = True

            # Wait for result
            time.sleep(3)
            for _ in range(10):
                try:
                    output.append(child.read_nonblocking(8192, timeout=0.5))
                except pexpect.TIMEOUT:
                    pass

            final = ''.join(output)
            if 'completed' in final.lower() or 'done' in final.lower() or 'failed' in final.lower():
                result["success"] = True
                print("[OK] Action processed")
        else:
            print(f"[FAIL] Pattern '{expected_pattern}' not found")
            # Debug output
            full = ''.join(output)
            for marker in ['Tool Approval', 'Question', 'Options', 'Write', 'Pending']:
                if marker in full:
                    print(f"  [DEBUG] Found: {marker}")

    except Exception as e:
        print(f"[ERROR] {e}")
    finally:
        child.sendcontrol('c')
        time.sleep(0.5)
        child.close()

    return result


def main():
    print("TUI INTEGRATION TESTS")
    print("=" * 60)
    print("Using pexpect to simulate terminal interaction")
    print()

    # Test 1: Approval Dialog (Approve)
    r1 = run_tui_test(
        "Approval Dialog - Approve",
        "Create a file /tmp/tui_test_approve.txt with hello",
        "Tool Approval",
        "\r",  # Enter to approve
        timeout_secs=50
    )

    # Test 2: Question Dialog
    r2 = run_tui_test(
        "Question Dialog",
        "I want to build something. Ask me what programming language I want to use.",
        "Question",
        "\r",  # Enter to select first option
        timeout_secs=50
    )

    # Test 3: Approval Dialog (Reject)
    r3 = run_tui_test(
        "Approval Dialog - Reject",
        "Create a file /tmp/tui_test_reject.txt with goodbye",
        "Tool Approval",
        "n",  # 'n' to reject
        timeout_secs=50
    )

    # Summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"Test 1 (Approve):  pattern={'PASS' if r1['found_pattern'] else 'FAIL'}, action={'PASS' if r1['action_taken'] else 'FAIL'}")
    print(f"Test 2 (Question): pattern={'PASS' if r2['found_pattern'] else 'FAIL'}, action={'PASS' if r2['action_taken'] else 'FAIL'}")
    print(f"Test 3 (Reject):   pattern={'PASS' if r3['found_pattern'] else 'FAIL'}, action={'PASS' if r3['action_taken'] else 'FAIL'}")

    all_passed = r1['found_pattern'] and r2['found_pattern'] and r3['found_pattern']
    print(f"\nOverall: {'PASS' if all_passed else 'PARTIAL/FAIL'}")

    return 0 if all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
