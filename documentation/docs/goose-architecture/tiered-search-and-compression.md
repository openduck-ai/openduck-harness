---
title: Tiered Search & Context Compression
sidebar_position: 5
description: Architecture for structured ripgrep code search and multi-tiered context compression defense lines.
---

# Tiered Search & Context Compression in OpenDuck

## Overview

In large-scale codebases (e.g., Monorepos, Rust/C++ workspaces, and multi-package JavaScript/TypeScript repositories), agentic coding workflows often suffer from two critical pain points:
1. **Context Window Token Explosion**: Unbounded full-text search results, build logs, and file dumps consume tens of thousands of tokens in a single turn.
2. **Attention Dilution ("Lost in the Middle")**: Flooding the LLM context with hundreds of lines of minified assets, lockfiles, or irrelevant search matches degrades reasoning quality and increases hallucination rates.

To address these challenges, OpenDuck incorporates a **Tiered Search & Context Compression** architecture. This system combines:
- **High-performance, rule-aware structured code search** (`grep_search`).
- **Multi-tiered context compression defense lines** (Tier 0 through Tier 3).
- **Zero-loss Protected Zones** for critical task states and user instructions.

---

## 1. Structured Code Search (`grep_search`)

Instead of relying on unstructured shell commands (e.g., raw `shell` running arbitrary `rg` or `grep` invocations), OpenDuck provides a dedicated `grep_search` tool within the `developer` platform extension.

### Key Capabilities

1. **Ripgrep Engine Integration**:
   - Built on Rust's `ignore::WalkBuilder` and `regex` crates.
   - Provides multi-threaded, parallel file traversal directly within the agent runtime without spawning external shell processes.
2. **Comprehensive Ignore Rules**:
   - Strictly respects `.gitignore`, `.git/info/exclude`, and global git configurations.
   - Automatically excludes VCS internal directories (e.g., `.git/`).
   - Automatically identifies and ignores binary files (via null-byte detection).
3. **`.ignore` Reverse Un-ignoring Support**:
   - If developers need the agent to inspect files or directories normally excluded by `.gitignore` (such as `node_modules/`, `dist/`, or vendor libraries), they can create an `.ignore` file in the project root with un-ignore syntax:
     ```gitignore
     # .ignore
     !node_modules/my-custom-package/
     !target/generated-sources/
     ```
   - OpenDuck's walker automatically prioritizes `.ignore` rules over `.gitignore`.
4. **Structured Parameters**:
   | Parameter | Type | Default | Description |
   | :--- | :--- | :--- | :--- |
   | `query` | `String` | *(Required)* | Search keyword or regular expression pattern. |
   | `path` | `Option<String>` | Workspace root | Target directory or file path to search within. |
   | `case_sensitive` | `Option<bool>` | `false` | Case-sensitive matching. |
   | `is_regex` | `Option<bool>` | `false` | Treat `query` as a regex pattern. |
   | `includes` | `Option<Vec<String>>` | `None` | Glob filter patterns (e.g., `["*.rs", "!**/tests/*"]`). |
   | `head_limit` | `Option<usize>` | `50` | Maximum matching lines to return (capped at 200). |
   | `context_lines` | `Option<usize>` | `0` | Number of context lines before/after match (0 to 5). |

5. **Head Limit & Structured Truncation**:
   When search results exceed `head_limit`, the output retains the first $N$ matches and appends an informative footer:
   ```text
   src/main.rs:12: pub fn start_service() {
   src/lib.rs:45: pub fn start_service() {
   
   [Found 84+ matches in 12 file(s). Showing first 50 matches (34+ omitted). Refine query or narrow search path.]
   ```

---

## 2. Multi-Tiered Context Compression

OpenDuck employs a 4-tier compression hierarchy to manage conversational context with minimal LLM overhead.

### Tier 0: Direct Raw Output
Outputs that are concise (below 12 KB and 60 lines) flow directly into the conversation stream without alteration.

### Tier 1: Zero-Cost Static Snip (`snip_tool_output`)
- **Location**: `crates/openduck-context-management/src/snip.rs`
- **Cost**: 0 tokens, 0 LLM calls, < 1ms CPU time.
- **Behavior**:
  - Retains head lines (default 40 lines) and tail lines (default 10 lines).
  - Replaces middle noise with: `[... X lines (Y bytes) snipped by OpenDuck context manager to save tokens ...]`.

### Tier 2: Tool-Pair Compaction (`ToolPairCompactionOperation`)
- **Location**: `crates/openduck/src/agents/state_machine/ops_tool_pair_compaction.rs`
- **Behavior**:
  - Automatically collapses completed tool request/response cycles into compact inline summaries while preserving reasoning flow.

### Tier 3: Global LLM Structured Compaction (`compact_messages`)
- **Location**: `crates/openduck-context-management/src/lib.rs`
- **Behavior**:
  - Triggered automatically when conversation context token usage reaches the compaction threshold (`GOOSE_AUTO_COMPACT_THRESHOLD`, default `0.80`).
  - Distills conversation history into a strongly-typed `StructuredSummary`.

---

## 3. Protected Zones Mechanism

To prevent aggressive compression from deleting vital mission parameters or active user constraints, OpenDuck enforces **Protected Zones**:
- **User Directives**: The original user prompt and recent user corrections are never discarded.
- **Active Tasks**: Task lists, Todo state, and required user confirmations remain intact.
- **Selective Eviction**: Only noisy intermediary tool outputs are pruned or summarized.

---

## 4. Verification and Testing

```bash
# 1. Run developer::grep unit tests
cargo test -p openduck --lib agents::platform_extensions::developer::grep

# 2. Run context management and Snip tests
cargo test -p openduck-context-management

# 3. Run full end-to-end grep integration tests
cargo test -p openduck --test grep_tests
```
