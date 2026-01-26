#!/bin/bash
#
# Claude Code Sandbox Runner
# Runs Claude Code with --dangerously-skip-permissions in an isolated Docker container
#
# Usage:
#   ./scripts/claude-sandbox.sh
#
# The script will interactively prompt you for:
#   1. Branch name (creates new branch or uses existing)
#   2. Confirmation before running
#
# All changes are isolated to the specified branch.
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
CONTAINER_NAME="claude-sandbox"
IMAGE_NAME="claude-sandbox-image"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}  Claude Code Sandbox Runner${NC}"
echo -e "${BLUE}================================${NC}"
echo ""
echo -e "${YELLOW}WARNING: This runs Claude with --dangerously-skip-permissions${NC}"
echo -e "${YELLOW}All changes will be isolated to a feature branch.${NC}"
echo ""

# Get current branch for reference
CURRENT_BRANCH=$(git branch --show-current)
echo -e "Current branch: ${BLUE}$CURRENT_BRANCH${NC}"
echo ""

# Prompt for branch name
echo -e "${GREEN}Enter branch name for this sandbox session:${NC}"
echo -e "(Leave empty to use current branch, or enter a new branch name)"
echo ""
read -p "Branch name: " BRANCH_INPUT

# Use input or fall back to current branch
if [[ -z "$BRANCH_INPUT" ]]; then
    BRANCH_NAME="$CURRENT_BRANCH"
    echo -e "${YELLOW}Using current branch: $BRANCH_NAME${NC}"
else
    BRANCH_NAME="$BRANCH_INPUT"
fi

# Confirm before proceeding
echo ""
echo -e "${YELLOW}You are about to run Claude with dangerous permissions on branch: ${GREEN}$BRANCH_NAME${NC}"
read -p "Continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${RED}Aborted.${NC}"
    exit 1
fi

echo ""

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}Error: Docker is not running. Please start Docker first.${NC}"
    exit 1
fi

# Check for uncommitted changes
if [[ -n $(git status --porcelain) ]]; then
    echo -e "${YELLOW}Warning: You have uncommitted changes.${NC}"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Create or switch to branch
if [[ "$BRANCH_NAME" != "$CURRENT_BRANCH" ]]; then
    echo -e "${BLUE}Switching to branch: ${BRANCH_NAME}${NC}"
    if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
        git checkout "$BRANCH_NAME"
    else
        echo -e "${YELLOW}Branch doesn't exist. Creating: ${BRANCH_NAME}${NC}"
        git checkout -b "$BRANCH_NAME"
    fi
fi

# Build the Docker image using inline Dockerfile
echo -e "${BLUE}Building Docker image...${NC}"
docker build -t "$IMAGE_NAME" - << 'DOCKERFILE'
FROM node:20-slim

# Install system dependencies
RUN apt-get update && apt-get install -y git curl && rm -rf /var/lib/apt/lists/*

# Create non-root user (Claude CLI refuses to run as root with dangerous mode)
RUN useradd -m -s /bin/bash claude

# Install Claude CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Set up claude user directories
RUN mkdir -p /home/claude/.claude && chown -R claude:claude /home/claude

# Switch to non-root user
USER claude
WORKDIR /app

# Default command - keep container running
CMD ["tail", "-f", "/dev/null"]
DOCKERFILE

# Stop and remove existing container if it exists
if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo -e "${YELLOW}Removing existing container...${NC}"
    docker rm -f "$CONTAINER_NAME" > /dev/null 2>&1
fi

# Prepare Claude auth directory
CLAUDE_AUTH_DIR="$HOME/.claude"
mkdir -p "$CLAUDE_AUTH_DIR"

# Run the container
echo -e "${BLUE}Starting container...${NC}"
docker run -d \
    --name "$CONTAINER_NAME" \
    -v "$PROJECT_DIR:/app" \
    -v "$CLAUDE_AUTH_DIR:/home/claude/.claude" \
    -w /app \
    -it \
    "$IMAGE_NAME"

echo ""
echo -e "${GREEN}Container started successfully!${NC}"
echo ""

# Check if Claude is authenticated
if ! docker exec "$CONTAINER_NAME" test -f /home/claude/.claude/.credentials.json 2>/dev/null; then
    echo -e "${YELLOW}Claude CLI not authenticated yet.${NC}"
    echo -e "${YELLOW}Running authentication...${NC}"
    echo ""
    docker exec -it "$CONTAINER_NAME" claude auth login
    echo ""
fi

# Show status
echo -e "${BLUE}================================${NC}"
echo -e "${GREEN}Ready to run Claude Code!${NC}"
echo -e "${BLUE}================================${NC}"
echo ""
echo -e "Branch: ${GREEN}$BRANCH_NAME${NC}"
echo -e "Container: ${GREEN}$CONTAINER_NAME${NC}"
echo ""

# Run Claude with dangerous permissions
echo -e "${BLUE}Entering interactive Claude session...${NC}"
echo -e "${YELLOW}Type your instructions or use /help for commands${NC}"
echo ""
docker exec -it "$CONTAINER_NAME" claude --dangerously-skip-permissions

# After Claude exits, show what changed
echo ""
echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}  Session Complete${NC}"
echo -e "${BLUE}================================${NC}"
echo ""

# Show git status
echo -e "${BLUE}Changes made:${NC}"
git status --short

echo ""
echo -e "${BLUE}Commands:${NC}"
echo -e "  ${GREEN}git diff${NC}                    - Review all changes"
echo -e "  ${GREEN}git add -A && git commit${NC}    - Commit changes"
echo -e "  ${GREEN}git checkout main${NC}           - Return to main branch"
echo -e "  ${GREEN}git merge $BRANCH_NAME${NC}      - Merge changes to main"
echo ""
echo -e "  ${YELLOW}./scripts/claude-sandbox.sh${NC} - Run another session"
echo ""

# Cleanup option
read -p "Stop and remove the container? (Y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Nn]$ ]]; then
    docker rm -f "$CONTAINER_NAME" > /dev/null 2>&1
    echo -e "${GREEN}Container removed.${NC}"
fi
