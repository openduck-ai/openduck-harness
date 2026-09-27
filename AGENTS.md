# AGENTS Instructions

OpenDuck is an AI Agent Harness and Enterprise Agent System in Rust with CLI, Web Hub dashboard, Electron desktop, and Mobile PWA interfaces.
The CLI binary is `openduck` (legacy aliases `goose` and `duck` still work). Workspace crates use the `openduck-*` prefix.

## Contribution Workflow

The issue is the source of truth for work intended for an upstream pull request. Track issue status on the [Goose Issues board](https://github.com/orgs/aaif-goose/projects/1).

- Before implementing an issue for a pull request, confirm that it is on the board with Status **Ready**.
- Do not implement issues in **Inbox**, **Needs info**, or **Accepted / design**. Help resolve the issue discussion instead.
- Read the agreed design, constraints, non-goals, and verification plan before changing code.
- Keep the implementation within the issue's agreed scope.
- If implementation reveals a material design change, return to the issue before continuing.
- Every external pull request must link the Ready issue it implements and explain how the verification plan was performed.
- Structure new issues on the matching template in `.github/ISSUE_TEMPLATE/` and set the issue type (e.g. Bug, Feature). `gh issue create` does not apply templates automatically.

Maintainer-directed work, urgent security fixes, release automation, and local or exploratory changes do not require a Ready issue.

## Agent Loop Migration

We are replacing the legacy agent loop in `crates/openduck/src/agents/agent.rs` with the state machine in `crates/openduck/src/agents/state_machine/`. The state-machine path is enabled with `OPENDUCK_STATE_MACHINE=1` (legacy `GOOSE_STATE_MACHINE=1` still works).

Until the migration is complete, changes to agent-loop behavior must be implemented and tested in both paths. When reviewing code, check whether a change to either path also applies to the other and flag missing parity.

## Setup
```bash
source bin/activate-hermit
cargo build
```

## Commands

### Build
```bash
cargo build                   # debug
cargo build --release         # release
just release-binary           # release binary
```

### Test
```bash
cargo test                    # all tests
cargo test -p openduck        # specific crate
cargo test -p openduck-harness # harness test suite
cargo test --package openduck --test mcp_integration_test
just record-mcp-tests         # record MCP
```

### Lint/Format
```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

### UI
```bash
# Web Hub (Project Dashboard & Harness UI)
cd ui/hub && pnpm run dev
cd ui/hub && pnpm test

# Desktop App
just run-ui                   # start desktop
cd ui/desktop && pnpm run typecheck
cd ui/desktop && pnpm test    # test UI

# Mobile PWA
cd ui/mobile && pnpm run dev
```

## Structure
```
crates/
├── openduck                 # core logic, control plane, scheduler, git, notifications
├── openduck-harness         # harness runtime, sandboxing, eval benchmarks, replay cassettes
├── openduck-agent           # pluggable agent policies and state machines
├── openduck-acp-macros      # ACP proc macros
├── openduck-cli             # CLI entry (`openduck`, plus legacy `goose` and `duck`)
├── openduck-context-management # compaction, pruning, and summarization
├── openduck-download-manager   # binary/asset download manager
├── openduck-local-inference # Candle-based local LLM runtime
├── openduck-mcp             # MCP extensions (agy, developer, computer controller, polymarket, memory)
├── openduck-providers       # LLM providers (Anthropic, OpenAI, Google, Ollama, Azure, Bedrock, etc.)
├── openduck-provider-types  # canonical provider request/response types
├── openduck-sdk             # Rust SDK & cross-language UniFFI bindings (Python, Kotlin/Maven, Swift)
├── openduck-test            # test utilities & integration tests
└── openduck-test-support    # test helpers & fixtures

ui/
├── hub/                     # OpenDuck Web Hub dashboard (React 19 + Vite)
├── hub-core/                # Shared REST & ACP client logic for Web Hub
├── desktop/                 # Electron desktop app
├── mobile/                  # Mobile PWA client
├── sdk/                     # TypeScript SDK (@openduck/sdk)
└── text/                    # Terminal UI (TUI) package
```

## Development Loop
```bash
# 1. source bin/activate-hermit
# 2. Make changes
# 3. cargo fmt
```

### Run these only if the user has asked you to build/test your changes:
```
# 1. cargo build
# 2. cargo test -p <crate>
# 3. cargo clippy --all-targets -- -D warnings
```

## Rules

- Test: Prefer tests/ folder, e.g. crates/openduck/tests/ or crates/openduck-harness/tests/
- Test: When adding features, update openduck-self-test.yaml, rebuild, then run `openduck run --recipe openduck-self-test.yaml` to validate
- Error: Use anyhow::Result
- Provider: Implement Provider trait see providers/base.rs
- MCP: Extensions in crates/openduck-mcp/
- UI Desktop: Use ACP SDK types or local `src/types/*` types. Do not import generated OpenAPI types/client code from `ui/desktop/src/api`

## Code Quality

- Comments: Write self-documenting code - prefer clear names over comments
- Comments: Never add comments that restate what code does
- Comments: Only comment for complex algorithms, non-obvious business logic, or "why" not "what"
- Simplicity: Don't make things optional that don't need to be - the compiler will enforce
- Simplicity: Booleans should default to false, not be optional
- Errors: Don't add error context that doesn't add useful information (e.g., `.context("Failed to X")` when error already says it failed)
- Simplicity: Avoid overly defensive code - trust Rust's type system
- Logging: Clean up existing logs, don't add more unless for errors or security events

## Never

- Never: Recreate `ui/desktop/src/api` or add `@hey-api/openapi-ts` to `ui/desktop`
- Cargo.toml: For human-authored dependency changes, use `cargo add` instead of manually editing dependency entries unless there is a specific reason not to.
- Cargo.toml: Automated dependency bump PRs are exempt; when manual edits are necessary, keep `Cargo.lock` consistent.
- Never: Skip cargo fmt
- Never: Merge without running clippy
- Never: Comment self-evident operations (`// Initialize`, `// Return result`), getters/setters, constructors, or standard Rust idioms
- Never: Overwrite a live binary in place (e.g. `cp`/`fs.copyFileSync` onto an existing executable) - unlink or atomic-rename the destination first, otherwise macOS SIGKILLs running processes with "Code Signature Invalid"

## Entry Points
- CLI: crates/openduck-cli/src/main.rs
- Harness: crates/openduck-harness/src/lib.rs
- Web Hub: ui/hub/src/main.tsx
- Desktop: ui/desktop/src/main.ts
- Agent: crates/openduck/src/agents/agent.rs
