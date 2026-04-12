#!/bin/bash
# GLM.sh — Launch GLM-5.1 Claude Code session for RusticV3
#
# Usage:
#   ~/GLM.sh "your prompt here"   — Run GLM-5.1 with a prompt (interactive, all perms)
#   ~/GLM.sh -p "your prompt"     — Run in print mode (non-interactive, returns output)
#   ~/GLM.sh                      — Start interactive session (no initial prompt)
#
# Hierarchy: Gemini < GLM-5.1 < Opus 4.6
# GLM-5.1 is the secondary implementer for RusticV3.

export API_TIMEOUT_MS=3000000
export ANTHROPIC_BASE_URL="https://api.z.ai/api/anthropic"
export ANTHROPIC_AUTH_TOKEN="736a923f979c4f8cb19499a06e348655.t4YsSJQeXxO42IYE"
export ANTHROPIC_DEFAULT_OPUS_MODEL="glm-5.1"
export ANTHROPIC_DEFAULT_SONNET_MODEL="glm-5-turbo"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="glm-4.7-flash"

# Unset CLAUDECODE to allow launching from within an existing Claude Code session
export CLAUDECODE=0

if [ "$1" = "-p" ]; then
    shift
    claude -p "$@" --model glm-5.1 --dangerously-skip-permissions
elif [ -n "$1" ]; then
    claude "$@" --model glm-5.1 --dangerously-skip-permissions
else
    claude --model glm-5.1 --dangerously-skip-permissions
fi
