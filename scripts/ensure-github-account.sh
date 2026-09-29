#!/bin/sh
# Run from the repository root. Never print or persist authentication tokens.
set -eu

account=Carlo1911
host=github.com

if ! command -v gh >/dev/null 2>&1; then
    echo "GitHub CLI (gh) is required. Install it and run: gh auth login --hostname $host" >&2
    exit 1
fi

current=$(gh api --hostname "$host" user --jq .login) || {
    echo "Cannot verify GitHub authentication. Run: gh auth login --hostname $host" >&2
    exit 1
}

if [ "$current" != "$account" ]; then
    echo "Switching GitHub account from $current to $account..."
    gh auth switch --hostname "$host" --user "$account" || {
        echo "Cannot switch to $account. Authenticate that account with: gh auth login --hostname $host" >&2
        exit 1
    }
    current=$(gh api --hostname "$host" user --jq .login)
fi

if [ "$current" != "$account" ]; then
    echo "Expected $account, but GitHub reports $current. Check GH_TOKEN / GITHUB_TOKEN environment overrides." >&2
    exit 1
fi

# Repository-local configuration only. Reset inherited helpers for this host,
# then delegate HTTPS authentication to gh's active account.
git config --local --replace-all "credential.https://$host.helper" ''
git config --local --add "credential.https://$host.helper" '!gh auth git-credential'
echo "GitHub account verified: $account (Git credentials configured for this repository)."
