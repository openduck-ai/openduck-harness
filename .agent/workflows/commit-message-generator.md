# Commit Message Generator

Use this workflow to analyze staged changes and generate a precise commit message.

## Context Constraints
* **CRITICAL**: You must ONLY consider staged changes. Do not look at unstaged modifications in the working tree.
* **Command**: Run `git diff --cached` to evaluate the diff.

## Formatting Style
* **Format**: Conventional Commits (e.g., `feat:`, `fix:`, `refactor:`, `docs:`)
* **Tone**: Imperative, concise, and professional.

