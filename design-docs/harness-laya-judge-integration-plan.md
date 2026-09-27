# OpenDuck Harness — Laya Local Judge Integration Plan

| Field | Value |
|---|---|
| **Title** | System 1 Local Decision Engine (Laya) Integration for OpenDuck Harness |
| **Status** | Proposed / Ready for Implementation |
| **Reference Project** | [qybaihe/mu](https://github.com/qybaihe/mu) (mu agent decision points & ledger) |
| **Model & Runtime** | `convaiinnovations/laya` (Local non-autoregressive System 1 decision engine) |
| **Integration Mode** | Sidecar HTTP Client (`http://localhost:8732/api/laya`) |
| **Target Crates** | `crates/openduck-harness`, `crates/openduck` |
| **Workspace** | `/mnt/e/goose` |

---

## 1. Executive Summary & Design Philosophy

### The Dual-Process Vision
Modern coding agents rely heavily on frontier autoregressive models (Claude 3.7 / GPT-4o / Gemini 2.5) for every single step. While these models excel at complex reasoning and code generation (System 2), using them for routine runtime checks introduces **multi-second latencies, high token costs, and vulnerability to loop stagnation**.

Following the architectural philosophy proven in **`qybaihe/mu`**:
> *"A coding agent that thinks before it acts. A small, fast judge makes the routine calls, the big model does the work."*

**Laya** is a local, lightweight (~421M parameter), non-autoregressive decision model. In a single forward pass (<40ms, 0 output tokens generated), it evaluates typed questions (`choice`, `score`, `noul`) over small contextual states with mathematically calibrated confidence probabilities.

```
                                  +---------------------------------------+
                                  |         Laya Sidecar (HTTP)           |
                                  |    http://localhost:8732/api/laya     |
                                  |  - Non-autoregressive System 1 (<40ms)|
                                  |  - Typed: choice, score, noul         |
                                  |  - Calibrated confidence probabilities|
                                  +---------------------------------------+
                                                     ▲
                                                     │ HTTP / JSON
                                                     ▼
+---------------------------------------------------------------------------------------------+
|                                      OPENDUCK-HARNESS                                       |
+---------------------------------------------------------------------------------------------+
|  [ Decision Points Engine (`crates/openduck-harness/src/judge/`) ]                          |
|   ├── Decision Modes: Off | Shadow (record only) | Active (governs execution)               |
|   └── Decision Ledger: Real-time telemetry, trajectory records, and latency tracking        |
+---------------------------------------------------------------------------------------------+
|  [ 1. Turn & Loop Guards ]                                                                  |
|   ├── `turn.drift`       -> Detect repetitive non-convergent loops (noul)                   |
|   ├── `turn.rewind`      -> Trigger snapshot rollback when stuck in dead-ends (noul)        |
|   └── `turn.completion`  -> Challenge unverified FinalAnswer claims before closing turn     |
+---------------------------------------------------------------------------------------------+
|  [ 2. Context & Compaction ]                                                                |
|   ├── `context.forget`   -> Triage stale tool output into one-line tombstones (choice)      |
|   └── `tool.admission`   -> Filter high-volume test outputs / diffs chunk-by-chunk          |
+---------------------------------------------------------------------------------------------+
|  [ 3. Safety & Permissions ]                                                                |
|   └── `tool.risk`        -> High-speed sanity check against user constraints (noul)         |
+---------------------------------------------------------------------------------------------+
```

---

## 2. Core Concepts Adopted from `qybaihe/mu`

### 2.1 Decision Points (决策点)
Every decision point is asked as a **short question about a compact local state**. The answer guides what the agent loop does next, without ever hallucinating free-form text:
- **`noul`**: Binary decision with a calibrated probability $[0.0, 1.0]$ and confidence measure (e.g. "Is this approach stuck?").
- **`choice`**: Categorical classification over discrete options (e.g. "Keep, prune, or fold this tool output?").
- **`score`**: Bounded rating levels.

### 2.2 Three-State Operating Modes (三态运行机制)
To ensure safety and facilitate continuous benchmarking, every decision point supports three operational states:
1. **`off`**: Bypassed entirely. Zero network overhead.
2. **`shadow`**: The question is posed to Laya and logged to the **Decision Ledger** alongside confidence scores and timestamps, but the execution flow continues unchanged. Used for validating Laya's calibration against real agent trajectories without risking task disruption.
3. **`active`**: Laya's verdict directly dictates runtime control flow (e.g. halting a stagnant turn, demanding verification tests, or dropping stale context).

### 2.3 Decision Ledger (决策账本)
All decisions (both shadow and active) are streamed into the harness's trajectory log (`TrajectoryStep`), capturing:
- Decision point key (e.g. `turn.drift`)
- Mode (`shadow` vs `active`)
- Input state digest
- Question type and criteria
- Verdict, probability distribution, and confidence
- Roundtrip latency in milliseconds

---

## 3. High-Priority Decision Points for OpenDuck

| Decision Point | Problem in OpenDuck Today | Laya Question Type | State & Action |
|:---|:---|:---|:---|
| **`turn.drift`** | Agent gets stuck in read-only loops (`cat`, `grep`, `ls`) repeating the same mistakes without advancing towards the goal. | `noul` | **State**: Last 3-5 tool invocations, exit codes, and initial task goal.<br>**Action**: If $P(\text{stagnant}) > 0.8$, trigger advisor consultation, warn agent, or break loop. |
| **`turn.completion`** | LLM outputs `FinalAnswer` prematurely without verifying code with tests or compiler checks. | `noul` | **State**: Task requirements vs last command outcomes.<br>**Action**: If unverified ($P(\text{verified}) < 0.35$), inject a system nudge: *"Verification required before declaring completion."* |
| **`context.forget`** | Full context compaction (`compact_context_messages`) uses heavy heuristics or LLM summaries when hitting watermark thresholds. | `choice` | **State**: Individual tool execution blocks (output > 1KB).<br>**Action**: Classify as `keep_active`, `collapse_to_tombstone`, or `truncate`. Replaces stale outputs with `[Tombstone: tool output archived]`. |
| **`tool.risk`** | Destructive commands (e.g. `rm -rf`, `git reset --hard`, database drops) rely solely on static regex rules. | `noul` | **State**: Proposed command string + user task description.<br>**Action**: Rapidly verify if the destructive action was explicitly intended by user prompt. |

---

## 4. Technical Architecture & Rust Implementation

### 4.1 Sidecar API Contract
Laya is hosted locally on `http://localhost:8732/api/laya`.

**Request payload:**
```json
{
  "state": "String or JSON describing the focused context",
  "questions": {
    "is_stagnant": {
      "type": "noul",
      "instructions": "Has the agent been in an unproductive or stagnant loop without making real progress?"
    },
    "action_route": {
      "type": "choice",
      "instructions": "What should the agent do next?",
      "criteria": {
        "retry": "Retry the same action with minor adjustment",
        "read_code": "Inspect surrounding code files",
        "escalate": "Ask human or abort"
      }
    }
  }
}
```

**Response payload:**
```json
{
  "model": "laya-rl-agent",
  "answers": {
    "is_stagnant": {
      "type": "noul",
      "noul": 0.8421,
      "confidence": 0.8421,
      "action": { "act_probability": 1.0 }
    },
    "action_route": {
      "type": "choice",
      "choice": "read_code",
      "probabilities": { "retry": 0.12, "read_code": 0.74, "escalate": 0.14 },
      "confidence": 0.62
    }
  },
  "usage": { "input_tokens": 128, "output_tokens": 0 },
  "routing": { "model": "typed-decisions" }
}
```

### 4.2 Module Structure in `openduck-harness`

```
crates/openduck-harness/src/
├── judge/
│   ├── mod.rs              # DecisionPoint, DecisionMode, DecisionLedger
│   ├── laya.rs             # LayaClient (HTTP client to http://localhost:8732/api/laya)
│   ├── points.rs           # Definitions for turn.drift, turn.completion, context.forget
│   └── ledger.rs           # Telemetry and trajectory persistence
├── runtime.rs              # Hook points in AgentHarness step execution loop
└── project/
    └── config.rs           # JudgeConfig in .openduck/harness.yaml
```

### 4.3 Configuration Schema (`harness.yaml`)
```yaml
judge:
  provider: laya
  endpoint: "http://localhost:8732/api/laya"
  timeout_ms: 350
  fallback_on_error: true
  points:
    turn.drift: shadow       # off | shadow | active
    turn.completion: shadow  # off | shadow | active
    context.forget: off      # off | shadow | active
    tool.risk: active        # off | shadow | active
```

---

## 5. Implementation Roadmap

### Phase 1: Client & Ledger Foundation (Days 1–2)
- [x] Create `crates/openduck-harness/src/judge/` module.
- [x] Implement `LayaClient` with connection pooling, configurable endpoint (`http://localhost:8732/api/laya`), and sub-350ms timeout.
- [x] Define `DecisionPoint`, `DecisionMode`, and `JudgmentRecord` structs.
- [x] Write integration test `crates/openduck-harness/tests/laya_judge_test.rs` asserting live responses against localhost:8732.

### Phase 2: Stagnation & Completion Guardrails (Days 3–4)
- [x] Implement `turn.drift` evaluation in `AgentHarness` tool execution loop (`runtime.rs`).
- [x] Implement `turn.completion` evaluation when `AgentAction::FinalAnswer` is received.
- [x] Enable `shadow` mode by default, recording judgments to `TrajectoryStep::judgments`.
- [x] Validate detection accuracy and regression tests with live Laya test suite.

### Phase 3: Context Compaction Integration (`context.forget`) (Days 5–6)
- [ ] Implement chunk-level evaluation in `openduck_context_management` / `runtime.rs`.
- [ ] If Laya identifies a historical tool output chunk as redundant ($P(\text{stale}) > 0.85$), replace it with a single-line tombstone.
- [ ] Measure token reduction and latency impact on SWE-bench style trajectories.

### Phase 4: Observability, Active Mode & Benchmarking (Day 7)
- [ ] Add CLI subcommand: `openduck harness judgments --task-id <ID>`.
- [ ] Conduct comparative evaluations: Baseline vs Shadow vs Active.
- [ ] Promote mature decision points (`turn.drift`, `turn.completion`) from `shadow` to `active`.
