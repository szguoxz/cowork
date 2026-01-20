#!/bin/bash
# Claude Code Loop Script
# Keeps Claude working on tasks and revisiting implementations until complete

set -e

# Configuration
MAX_ITERATIONS=${MAX_ITERATIONS:-10}
TASK_FILE="${TASK_FILE:-tasks.md}"
LOG_DIR="logs/claude-loop"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create log directory
mkdir -p "$LOG_DIR"

# Log file for this session
SESSION_LOG="$LOG_DIR/session_$TIMESTAMP.log"

log() {
    local level=$1
    shift
    local message="$*"
    local timestamp=$(date +"%Y-%m-%d %H:%M:%S")
    echo -e "[$timestamp] [$level] $message" | tee -a "$SESSION_LOG"
}

print_header() {
    echo -e "${BLUE}"
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║           Claude Code Iterative Development Loop             ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

show_usage() {
    echo "Usage: $0 [OPTIONS] [TASK_DESCRIPTION]"
    echo ""
    echo "Options:"
    echo "  -f, --file FILE      Read tasks from FILE (default: tasks.md)"
    echo "  -n, --max-iter N     Maximum iterations (default: 10)"
    echo "  -c, --continue       Continue from previous session"
    echo "  -v, --verify-only    Only verify existing implementations"
    echo "  -h, --help           Show this help message"
    echo ""
    echo "Environment Variables:"
    echo "  MAX_ITERATIONS       Maximum loop iterations"
    echo "  TASK_FILE            File containing tasks"
    echo ""
    echo "Examples:"
    echo "  $0 'Implement user authentication with tests'"
    echo "  $0 -f features.md -n 5"
    echo "  $0 --verify-only"
}

# Parse command line arguments
VERIFY_ONLY=false
CONTINUE_SESSION=false
TASK_DESCRIPTION=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -f|--file)
            TASK_FILE="$2"
            shift 2
            ;;
        -n|--max-iter)
            MAX_ITERATIONS="$2"
            shift 2
            ;;
        -c|--continue)
            CONTINUE_SESSION=true
            shift
            ;;
        -v|--verify-only)
            VERIFY_ONLY=true
            shift
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            TASK_DESCRIPTION="$1"
            shift
            ;;
    esac
done

# Build the prompt for Claude
build_prompt() {
    local iteration=$1
    local mode=$2

    if [[ "$mode" == "verify" ]]; then
        cat << 'PROMPT'
Review and verify all recently implemented features:

1. Run the test suite and check for failures
2. Review the code for bugs, edge cases, and security issues
3. Verify the implementation matches the requirements
4. Check for any TODO comments or incomplete implementations
5. Run clippy/linting and fix any warnings

If you find ANY issues:
- Fix them immediately
- Re-run tests to confirm the fix
- Update the todo list

Report your findings with:
- VERIFIED: [feature] - if working correctly
- FIXED: [feature] - [what was wrong] - if you fixed something
- NEEDS_WORK: [feature] - [what's missing] - if more work needed

Be thorough - check edge cases, error handling, and integration points.
PROMPT
    elif [[ -n "$TASK_DESCRIPTION" ]]; then
        cat << PROMPT
Iteration $iteration of $MAX_ITERATIONS

TASK: $TASK_DESCRIPTION

Instructions:
1. If this is the first iteration, plan the implementation and create a todo list
2. Work through the tasks systematically
3. Write tests for each feature BEFORE or DURING implementation
4. Run tests frequently to catch issues early
5. Mark tasks complete only when tests pass

After completing work, summarize:
- COMPLETED: [list of completed items]
- IN_PROGRESS: [items still being worked on]
- BLOCKED: [any blockers encountered]
- NEXT: [what should be done next iteration]

Focus on quality over speed. It's better to complete fewer tasks correctly.
PROMPT
    elif [[ -f "$TASK_FILE" ]]; then
        cat << PROMPT
Iteration $iteration of $MAX_ITERATIONS

Read the task file at: $TASK_FILE

Instructions:
1. Review the tasks/features listed in the file
2. Create or update your todo list based on remaining work
3. Implement features with tests
4. Mark completed items in your todo list
5. Update the task file if needed to track progress

After completing work, summarize:
- COMPLETED: [list of completed items]
- IN_PROGRESS: [items still being worked on]
- BLOCKED: [any blockers encountered]
- NEXT: [what should be done next iteration]

Focus on quality over speed. It's better to complete fewer tasks correctly.
PROMPT
    else
        echo "Error: No task description or task file provided"
        exit 1
    fi
}

# Check if all tasks appear complete
check_completion() {
    local output_file=$1

    # Look for indicators that work is complete
    if grep -q "NEEDS_WORK:" "$output_file" 2>/dev/null; then
        return 1  # Not complete
    fi

    if grep -q "BLOCKED:" "$output_file" 2>/dev/null; then
        local blocked_content=$(grep "BLOCKED:" "$output_file")
        if [[ ! "$blocked_content" =~ "BLOCKED: None" ]] && [[ ! "$blocked_content" =~ "BLOCKED:$" ]]; then
            log "WARN" "Blocked items found"
            return 1
        fi
    fi

    if grep -q "IN_PROGRESS:" "$output_file" 2>/dev/null; then
        local in_progress=$(grep "IN_PROGRESS:" "$output_file")
        if [[ ! "$in_progress" =~ "IN_PROGRESS: None" ]] && [[ ! "$in_progress" =~ "IN_PROGRESS:$" ]]; then
            return 1  # Still in progress
        fi
    fi

    # Check for test failures
    if grep -qi "test.*fail\|failed\|error\|panic" "$output_file" 2>/dev/null; then
        if ! grep -qi "fixed\|resolved\|passing" "$output_file" 2>/dev/null; then
            return 1
        fi
    fi

    return 0  # Appears complete
}

# Main loop
run_loop() {
    print_header

    log "INFO" "Starting Claude Code loop"
    log "INFO" "Max iterations: $MAX_ITERATIONS"
    log "INFO" "Session log: $SESSION_LOG"

    local iteration=1
    local consecutive_complete=0
    local required_verifications=2  # Need 2 consecutive "complete" results

    while [[ $iteration -le $MAX_ITERATIONS ]]; do
        echo ""
        echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
        echo -e "${YELLOW}  Iteration $iteration of $MAX_ITERATIONS${NC}"
        echo -e "${YELLOW}═══════════════════════════════════════════════════════════════${NC}"
        echo ""

        local output_file="$LOG_DIR/iteration_${iteration}_$TIMESTAMP.md"
        local mode="implement"

        # Every 3rd iteration or if verify-only, do verification
        if [[ "$VERIFY_ONLY" == "true" ]] || [[ $((iteration % 3)) -eq 0 ]]; then
            mode="verify"
            log "INFO" "Running verification pass"
        fi

        local prompt=$(build_prompt $iteration $mode)

        log "INFO" "Running Claude Code (mode: $mode)..."

        # Run Claude Code with the prompt
        # Using --print to get output, piping to tee for logging
        if echo "$prompt" | claude --dangerously-skip-permissions --print 2>&1 | tee "$output_file"; then
            log "INFO" "Claude Code completed iteration $iteration"
        else
            log "ERROR" "Claude Code encountered an error"
            # Continue anyway, might be recoverable
        fi

        # Check if work appears complete
        if check_completion "$output_file"; then
            ((consecutive_complete++))
            log "INFO" "Work appears complete (verification $consecutive_complete/$required_verifications)"

            if [[ $consecutive_complete -ge $required_verifications ]]; then
                echo ""
                echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
                echo -e "${GREEN}║                    ALL TASKS COMPLETE!                       ║${NC}"
                echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
                log "INFO" "All tasks verified complete after $iteration iterations"
                return 0
            fi
        else
            consecutive_complete=0
            log "INFO" "Work still in progress"
        fi

        ((iteration++))

        # Brief pause between iterations
        if [[ $iteration -le $MAX_ITERATIONS ]]; then
            echo ""
            log "INFO" "Pausing before next iteration..."
            sleep 2
        fi
    done

    echo ""
    echo -e "${YELLOW}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${YELLOW}║          MAX ITERATIONS REACHED - Review needed              ║${NC}"
    echo -e "${YELLOW}╚══════════════════════════════════════════════════════════════╝${NC}"
    log "WARN" "Reached maximum iterations ($MAX_ITERATIONS)"

    return 1
}

# Entry point
main() {
    # Check if claude command is available
    if ! command -v claude &> /dev/null; then
        echo -e "${RED}Error: 'claude' command not found${NC}"
        echo "Please install Claude Code first"
        exit 1
    fi

    # Run the loop
    run_loop
    exit_code=$?

    echo ""
    log "INFO" "Session complete. Logs saved to: $LOG_DIR"

    exit $exit_code
}

main
