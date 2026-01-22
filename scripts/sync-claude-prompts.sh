#!/bin/bash
# Sync Claude Code Prompts
#
# This script manages the Claude Code prompt system for Cowork.
# Since Claude Code prompts are not publicly available, this script:
# 1. Creates the directory structure
# 2. Validates existing prompts have no unexpanded variables
# 3. Records sync metadata
#
# Usage:
#   ./scripts/sync-claude-prompts.sh          # Initial setup or validation
#   ./scripts/sync-claude-prompts.sh --check  # Dry run validation only
#   ./scripts/sync-claude-prompts.sh --force  # Force re-create structure

set -euo pipefail

# Configuration
OUTPUT_DIR="crates/cowork-core/src/prompt/builtin/claude_code"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Allowed runtime variables (these should NOT be expanded at build time)
RUNTIME_VARS=(
    "WORKING_DIRECTORY"
    "IS_GIT_REPO"
    "GIT_STATUS"
    "CURRENT_BRANCH"
    "MAIN_BRANCH"
    "CURRENT_DATE"
    "CURRENT_YEAR"
    "PLATFORM"
    "OS_VERSION"
    "MODEL_INFO"
    "AGENT_TYPE_REGISTRY_STRING"
    "LIMITED_COMMANDS"
    "RECENT_COMMITS"
    "SECURITY_POLICY"
    "ASSISTANT_NAME"
    "SKILLS_XML"
    "MCP_SERVER_INSTRUCTIONS"
)

# Function: Print with color
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function: Create directory structure
create_structure() {
    log_info "Creating directory structure in $OUTPUT_DIR..."

    cd "$PROJECT_ROOT"

    mkdir -p "$OUTPUT_DIR"/{system,tools,agents,reminders}

    log_info "Directory structure created."
}

# Function: Build regex pattern for allowed runtime variables
build_allowed_pattern() {
    local pattern=""
    for var in "${RUNTIME_VARS[@]}"; do
        if [[ -n "$pattern" ]]; then
            pattern="$pattern|"
        fi
        pattern="$pattern$var"
    done
    echo "$pattern"
}

# Function: Validate no unexpanded build-time variables
validate_expansion() {
    log_info "Validating expanded prompts..."

    cd "$PROJECT_ROOT"

    local errors=0
    local allowed_pattern
    allowed_pattern=$(build_allowed_pattern)

    # Check for unexpanded tool name variables (build-time)
    if grep -rE '\$\{[A-Z_]+_TOOL' "$OUTPUT_DIR" 2>/dev/null; then
        log_error "Found unexpanded tool variables"
        errors=$((errors + 1))
    fi

    # Check for unexpanded function calls (build-time)
    if grep -rE '\$\{[A-Z_]+\(\)' "$OUTPUT_DIR" 2>/dev/null; then
        log_error "Found unexpanded function calls"
        errors=$((errors + 1))
    fi

    # Check for unexpanded object property access (build-time)
    if grep -rE '\$\{[A-Z_]+\.[a-z]+' "$OUTPUT_DIR" 2>/dev/null; then
        log_error "Found unexpanded object property accesses"
        errors=$((errors + 1))
    fi

    # Check for any variables NOT in the allowed list
    # This uses a negative lookahead pattern
    local other_vars
    other_vars=$(grep -roE '\$\{[A-Z_]+\}' "$OUTPUT_DIR" 2>/dev/null | \
                 grep -vE "\\\$\{($allowed_pattern)\}" || true)

    if [[ -n "$other_vars" ]]; then
        log_error "Found unexpanded variables not in allowed list:"
        echo "$other_vars"
        errors=$((errors + 1))
    fi

    if [[ $errors -eq 0 ]]; then
        log_info "Validation passed! All build-time variables have been expanded."
        return 0
    else
        log_error "Validation failed with $errors error(s)"
        return 1
    fi
}

# Function: Record sync metadata
record_metadata() {
    log_info "Recording sync metadata..."

    cd "$PROJECT_ROOT"

    local sync_date
    sync_date=$(date -Iseconds 2>/dev/null || date +%Y-%m-%dT%H:%M:%S%z)

    cat > "$OUTPUT_DIR/SYNC_INFO.md" << EOF
# Claude Code Prompt Sync Information

## About

This directory contains prompts adapted from Claude Code's prompt system.
The prompts have been pre-expanded to replace build-time template variables
with their literal values.

## Sync Details

- **Synced**: $sync_date
- **Script**: \`./scripts/sync-claude-prompts.sh\`

## Runtime Variables

The following variables are substituted at runtime by \`TemplateVars\`:

| Variable | Description |
|----------|-------------|
| \`\${WORKING_DIRECTORY}\` | Current working directory |
| \`\${IS_GIT_REPO}\` | Whether the directory is a git repo |
| \`\${GIT_STATUS}\` | Git status output |
| \`\${CURRENT_BRANCH}\` | Current git branch |
| \`\${MAIN_BRANCH}\` | Main/master branch name |
| \`\${CURRENT_DATE}\` | Today's date |
| \`\${CURRENT_YEAR}\` | Current year |
| \`\${PLATFORM}\` | Operating system (linux, macos, windows) |
| \`\${OS_VERSION}\` | OS version string |
| \`\${MODEL_INFO}\` | Model name and ID |
| \`\${ASSISTANT_NAME}\` | Assistant name (e.g., "Cowork") |
| \`\${RECENT_COMMITS}\` | Recent git commit log |
| \`\${SECURITY_POLICY}\` | Security policy content |
| \`\${SKILLS_XML}\` | Available skills as XML |
| \`\${MCP_SERVER_INSTRUCTIONS}\` | MCP server instructions |

## Maintenance

To validate prompts:

\`\`\`bash
./scripts/sync-claude-prompts.sh --check
\`\`\`

To rebuild:

\`\`\`bash
cargo build
cargo test -p cowork-core prompt::builtin::claude_code
\`\`\`
EOF

    log_info "Metadata recorded to $OUTPUT_DIR/SYNC_INFO.md"
}

# Function: Show help
show_help() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Manage Claude Code prompts for Cowork.

OPTIONS:
    --check     Validate existing prompts without modifying
    --force     Force re-create directory structure
    --help      Show this help message

EXAMPLES:
    $(basename "$0")              # Initial setup or validation
    $(basename "$0") --check      # Dry run validation only
    $(basename "$0") --force      # Force re-create structure
EOF
}

# Main function
main() {
    local check_only=false
    local force=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --check)
                check_only=true
                shift
                ;;
            --force)
                force=true
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done

    cd "$PROJECT_ROOT"

    # Check only mode
    if $check_only; then
        log_info "Running validation only..."
        if [[ -d "$OUTPUT_DIR" ]]; then
            validate_expansion
        else
            log_warn "Directory $OUTPUT_DIR does not exist. Run without --check first."
            exit 1
        fi
        exit 0
    fi

    # Create or update structure
    if [[ ! -d "$OUTPUT_DIR" ]] || $force; then
        create_structure
    else
        log_info "Directory exists. Use --force to recreate."
    fi

    # Validate if content exists
    if find "$OUTPUT_DIR" -name "*.md" -type f 2>/dev/null | grep -q .; then
        validate_expansion || true  # Don't fail on validation errors during setup
    else
        log_info "No .md files found yet. Skipping validation."
    fi

    # Record metadata
    record_metadata

    log_info "Done! Run 'cargo build' to rebuild with prompts."
}

main "$@"
