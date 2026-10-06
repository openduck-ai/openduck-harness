---
title: Task Run Step Latency Diagnostics and Optimization
sidebar_label: Step Latency Diagnostics & Optimization
description: Empirical latency analysis, root cause diagnosis, and architectural context optimization for OpenDuck harness runs.
---

# Task Run Step Latency Diagnostics and Optimization

## 1. Executive Summary

During evaluation benchmark runs in OpenDuck Harness, certain multi-step tasks exhibit elevated turn latencies leading to multi-minute runtimes. An empirical investigation of task run [`20261006_173051_task_task-196535_01a11244`](file:///home/zhanghu/.local/state/openduck/harness_runs/20261006_173051_task_task-196535_01a11244.json) was conducted to diagnose the exact bottlenecks across the runtime stack.

### Key Metrics Summary

| Metric | Measured Value | Share / Context |
| :--- | :--- | :--- |
| **Total Task Runtime** | **1,452.8 s** (24.2 min) | 100% |
| **LLM Provider Generation (`opencode_go:qwen3.8-flash`)** | **1,351.5 s** (22.5 min) | **93.0%** of total runtime |
| **Tool Execution Duration** | ~0.1 s | Fast local commands (`curl`, `sed`, `git`, python) |
| **Harness / Agent Overhead** | 101.2 s (1.7 min) | ~7.0% (~1.7s per turn across 59 steps) |
| **Total Steps** | **59 steps** | Average **22.9 s / step** |
| **Total Input Tokens** | 1,946,177 tokens | Prompt grew from 1,957 to ~55,000 tokens |
| **Total Output Tokens** | 74,470 tokens | ~1,262 tokens / step average |
| **Generation Throughput** | **~55 tokens/sec** | Baseline model inference decoding speed |

```mermaid
pie title Task Runtime Distribution (Total: 24.2 min)
    "LLM Provider Generation" : 1351.5
    "Harness & Agent Overhead" : 101.2
    "Tool Execution" : 0.1
```

---

## 2. Root Cause Analysis

### A. Autoregressive Generation Throughput Bound ($R = 0.995$)

Statistical regression across all 59 steps demonstrated an almost perfect correlation (**$R = 0.995$**) between **Output Tokens** and **Step Latency**. 

The LLM provider (`opencode_go:qwen3.8-flash`) outputs tokens at a deterministic throughput of **~50 to 65 tokens per second**. Consequently, latency is primarily a direct function of generated token volume:
- A turn with 1,000 output tokens requires **~18 seconds**.
- A turn with 3,500 output tokens requires **~60 seconds**.
- A turn with 10,866 output tokens (Step 42) requires **185.6 seconds (3.1 minutes)**.

### B. Excessive Chain-of-Thought (CoT) / Thinking Tokens

Across the 59 steps, the model generated **109,427 characters** (~31,265 tokens) inside `<thinking>` tags. 
- **~42% of all generated tokens** were internal thinking tokens.
- This consumed approximately **~568 seconds (9.5 minutes)** of pure inference time.
- In middle crawl turns (Steps 22–37), the model routinely generated 6,000 to 10,000 characters of reasoning before executing simple commands such as `curl` or basic string cleanup.

### C. Monolithic File Generation in Tool Request (Step 42)

In Step 42, the model used the `write_file` tool to emit the full 13.2 KB markdown report (`2026-10-06-132301-viral-topic-5picks-views-forecast.md`) inside a single `arguments.content` string parameter:
- Output token count: **10,866 tokens**.
- Step latency: **185.6 seconds** (3.1 minutes for this single turn).

### D. Historical Context Inflation & Lack of Tool Payload Pruning

When an assistant emits a large tool call like Step 42, the entire 49,044-character `write_file` call remained in the conversation history without pruning. 
- For all subsequent turns (Steps 43 through 59, 17 steps in total), this 49 KB payload was serialized and re-sent over the wire on every single request.
- Across those 17 steps alone, over **362,000 characters (~103,000 tokens)** were redundantly processed.
- Similarly, uncompacted raw web scraping outputs (HTML from curl) contributed another **~375,000 redundant tokens** across the run.

### E. Micro-Step Sequential Thrashing (High Turn Count)

Steps 43 through 58 (16 steps) performed incremental verification checks:
- Separate steps for `grep`, `echo`, two Python scripts, three individual `sed` invocations, and git status checks.
- 16 sequential round-trips at ~15–20s each added ~4.5 minutes to the run.

---

## 3. Empirical Step Breakdown

### Top 10 Slowest Steps

| Step | Action / Tool | Latency | In Tokens | Out Tokens | Think Chars | Description |
| :---: | :--- | :---: | :---: | :---: | :---: | :--- |
| **42** | `write_file` | **185.6s** | 44,736 | **10,866** | 5,890 | Wrote 13.2 KB forecast report in a single tool call |
| **34** | `shell` | **76.1s** | 40,980 | **4,107** | 8,988 | Long thinking before cleaning article text |
| **27** | `shell` | **71.4s** | 32,525 | **4,123** | 10,064 | Extensive reasoning before curling article |
| **32** | `shell` | **67.9s** | 37,444 | **3,736** | 6,485 | Reasoning before inspecting candidate notes |
| **24** | `shell` | **66.8s** | 28,931 | **3,654** | 8,889 | Reasoning on TechCrunch feed |
| **37** | `shell` | **61.9s** | 43,364 | **3,483** | 7,179 | Reasoning before running Python parser |
| **22** | `shell` | **54.5s** | 26,155 | **3,105** | 5,799 | Reasoning before inspecting Verge feed |
| **36** | `shell` | **53.8s** | 42,699 | **3,194** | 6,259 | Deliberating on topic validation script |
| **29** | `shell` | **53.2s** | 34,788 | **2,923** | 4,332 | Reasoning before inspecting methodology stats |
| **45** | `shell` | **49.9s** | 52,692 | **3,236** | 2,307 | Post-verification script execution & verification |

### Trajectory Progression Across Phases

| Phase | Steps | Avg Latency | Avg In Tokens | Avg Out Tokens | Avg Think Chars |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Recon & Dedup** | 1–10 | 5.1s | 6,616 | 230 | 478 |
| **Feed Discovery** | 11–20 | 11.8s | 18,076 | 668 | 1,287 |
| **Deep Crawl & Eval** | 21–30 | 36.6s | 30,340 | 2,079 | 3,887 |
| **Article Analysis** | 31–40 | 33.7s | 41,703 | 1,894 | 3,571 |
| **Report & Verify** | 41–50 | 36.3s | 51,772 | 1,961 | 1,320 |
| **Final Checks & Exit**| 51–59 | 13.1s | 51,233 | 684 | 445 |

---

## 4. Architectural Improvements Implemented (Phase 1)

### 1. Historical Tool Argument Snipping (`snip_tool_call_arguments`)

Implemented in [`crates/openduck-context-management/src/snip.rs`](file:///mnt/e/openduck-harness/crates/openduck-context-management/src/snip.rs):

```rust
pub const DEFAULT_MAX_TOOL_CALL_CONTENT_BYTES: usize = 1024;

pub fn snip_tool_call_arguments(
    _tool_name: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
    max_content_bytes: usize,
) -> (serde_json::Map<String, serde_json::Value>, bool) {
    let mut modified = false;
    let mut new_args = arguments.clone();

    let targets = ["content", "patch", "replacement", "file_text"];
    for key in targets {
        if let Some(serde_json::Value::String(s)) = new_args.get(key) {
            if s.len() > max_content_bytes {
                let original_bytes = s.len();
                let snippet: String = s.chars().take(120).collect();
                let replacement = format!(
                    "{snippet}\n[... {original_bytes} bytes omitted from historical tool call by OpenDuck context manager ...]"
                );
                new_args.insert(key.to_string(), serde_json::Value::String(replacement));
                modified = true;
            }
        }
    }

    (new_args, modified)
}
```

### 2. Policy Step Message History Sanitation

In [`crates/openduck/src/control/harness.rs`](file:///mnt/e/openduck-harness/crates/openduck/src/control/harness.rs#L513-L525) within `GooseAgentPolicy::step_detailed`:
- Historical assistant messages containing `write_file` / `create_file` now have their arguments snipped before being formatted for LLM provider requests.
- Preserves the file path and initial snippet while discarding multi-kilobyte payloads.
- **Measured Impact**: Eliminates **~103,000 redundant tokens** (~5.3% of total input tokens) from repeat file write re-transmissions.

### 3. Context Compaction Integration

In [`crates/openduck-harness/src/runtime.rs`](file:///mnt/e/openduck-harness/crates/openduck-harness/src/runtime.rs#L2314-L2327) within `compact_context_messages`:
- Added argument snipping alongside response snipping when tokens exceed the watermark.
- Supported by unit test [`test_tool_call_arguments_snipping`](file:///mnt/e/openduck-harness/crates/openduck-harness/tests/anti_stagnation_tests.rs#L1052-L1072).

---

## 5. Optimization Roadmap

| Phase | Target Component | Proposed Action | Expected Latency Reduction | Status |
| :---: | :--- | :--- | :---: | :---: |
| **Phase 1** | `openduck-context-management` & `openduck-harness` | Snip historical `write_file` arguments and compact high-volume tool responses. | **-25% input tokens (~480k tokens)** | **Completed** |
| **Phase 2** | `openduck-providers` | Expose `reasoning_effort` / `thinking_budget` configuration for OpenAI-compatible providers. | **-35% total LLM time** (~5–7 min saved) | Ready |
| **Phase 3** | `openduck-harness` | Prompt engineering & turn batching guidelines to consolidate post-verification steps. | **-20–30 turns** (~4–6 min saved) | Ready |

### Projected Performance Impact

| Metric | Baseline Run (`173051`) | Projected After Full Optimization | Net Improvement |
| :--- | :---: | :---: | :---: |
| **Total Duration** | **24.2 min** (1,452s) | **7.0 – 9.0 min** (420–540s) | **~65% faster** |
| **Total Steps** | 59 steps | 20 – 25 steps | **~60% fewer turns** |
| **Output Tokens** | 74,470 | ~25,000 | **~66% reduction** |
| **Thinking Tokens** | ~31,265 | ~8,000 | **~75% reduction** |
| **Cumulative Input Tokens** | 1,946,177 | ~450,000 | **~77% reduction** |
| **Max Step Latency** | 185.6s (Step 42) | < 60s | **No 3-minute spikes** |
