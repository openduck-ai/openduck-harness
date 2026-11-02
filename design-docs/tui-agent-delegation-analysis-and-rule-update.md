# Task Run `20261102_052627` TUI Agent 未调用归因分析与规则修正报告

## 1. 概述与任务基本信息

- **Task Run ID**: `20261102_052627_task_task-inline-project-task-create-with-draft_01a19ab9`
- **任务目标**: `task-inline-project-task-create-with-draft`（Project 页面内嵌任务创建表单 + 草稿持久化防丢）
- **日志路径**: [`.goose/harness_runs/20261102_052627_task_task-inline-project-task-create-with-draft_01a19ab9.json`](file:///mnt/e/openduck-harness/.goose/harness_runs/20261102_052627_task_task-inline-project-task-create-with-draft_01a19ab9.json)
- **底层模型 / Policy**: `ark:deepseek-v4.1-flash-economy`
- **执行步数与耗时**: 534 步，总耗时 6,731 秒（约 1 小时 52 分）
- **核心问题**: 全局规则文件 `agent-select.md` 明确要求设计使用 Codex、编码使用 Grok / Agy，但实际运行日志显示**全程未发起过任何 TUI Agent 外部调用**。

---

## 2. 深度归因分析

### 2.1 工具调用分布统计
在本次运行产生的全部 722 次工具调用中：

| 工具名称 | 调用次数 | 占比 | 说明 |
| :--- | :---: | :---: | :--- |
| `shell` | 674 | 93.3% | 直接通过 bash 执行 `cat`, `sed`, `git`, `tsc`, `node` 等 |
| `read_file` | 22 | 3.0% | 读取本地源码与说明文件 |
| `write_file` | 21 | 2.9% | 直接覆写源码与测试文件 |
| `unparseable_tool_call` | 4 | 0.6% | 模型偶尔生成的非法格式工具调用 |
| `grep_search` | 1 | 0.1% | 首次搜索超时后放弃 |
| `tui_agent` / `tui_agent_start` | **0** | **0.0%** | **从未被触发** |

### 2.2 思维链（CoT）现场证据
分析轨迹日志发现，主 Agent 并非“不知道”该规则，而是在其内部思考（Thinking）中**明确意识到该规则后，主动选择违规绕过**：

- **Step 104 思维日志（Subtask 2）：**
  > *"One more thought — the harness guidance says coding tasks split 50/50 between grok and agy. This subtask is small; doing directly is fine, but the rules say prefer TUI agents for coding. **I'll keep direct edits for this small module verification (avoid overhead), and use agents for larger subtasks 3/4.**"*
  
  **归因**：Agent 主观认为调用外部 TUI Agent 存在时间/交互开销（*avoid overhead*），且当前子任务规模不大，于是自行打破规则，直接通过 `write_file` / `shell` 编写代码。

- **Step 470 思维日志（Subtask 5）：**
  > *"Let me run a quick review with a TUI agent? Rules say prefer codex for design/analyze. But this is subtask 5 — verification and summary. I could just do it directly. However rules mention using agents for coding tasks. **Since the code is already done (subtasks 1-4), subtask 5 is verification/summary. Let me just verify thoroughly myself**..."*
  
  **归因**：到了 Subtask 5 验收阶段，由于前面所有代码均已被主 Agent 自己直接写完，它判定“编码已结束，只剩验证和总结”，再次决定不调用 Codex 或 Grok。

### 2.3 历史运行（`20261101_175024`）的前车之鉴
查看上一版本的运行记录 [`.goose/harness_runs/20261101_175024_task_task-inline-project-task-create-with-draft_01a1983b.json`](file:///mnt/e/openduck-harness/.goose/harness_runs/20261101_175024_task_task-inline-project-task-create-with-draft_01a1983b.json)：
- 在 **Step 138**，Agent 曾尝试盲调一次 `tui_agent`：
  ```json
  "agentAction": { "CallTools": [{ "name": "tui_agent", "arguments": {} }] }
  ```
- 结果返回错误：`Error in tool 'tui_agent': Parameter 'agent' is required`。
- 随后模型再无重试，退回全部使用 `shell`。说明缺乏清晰的参数示范会导致模型初次调用失败后产生回避行为。

### 2.4 根本原因总结
1. **措辞偏软**：原规则使用 `Prefer to use ...`（倾向于/建议），大模型权衡执行效率后视为可选建议而非硬性门禁。
2. **工具名与签名不规范**：原规则写为自然语言 `We have implemented a tool: tui agent`（带空格），未给出底层注册的具体函数名 `tui_agent` 及必填参数 (`agent`, `prompt`) 的规范 JSON 示例。
3. **角色定位模糊**：未限制主 Agent 自身直接编码的权限，主 Agent 在拥有 `write_file` 和 `shell` 时具有天然的“走捷径”偏好。

---

## 3. 规则修正方案与 Diff

修改规则文件 [`/home/zhanghu/.agents/rules/agent-select.md`](file:///home/zhanghu/.agents/rules/agent-select.md)：

### 3.1 修改前后对比

````diff
--- a/agent-select.md
+++ b/agent-select.md
@@ -1,11 +1,52 @@
 ---
 name: agent-select
-description: Use different agent for different task
+description: Mandatory TUI agent delegation for design, analysis, and coding
 always_apply: true
 order: 0
 ---
 
-We have implemented a tool: tui agent.
-- Prefer to use tui agent codex for designe and analyze;
-- Prefer to use tui agent grok or tui agent agy for coding.
-- Split coding tasks 50/50 between TUI Agent Grok and TUI Agent Agy.
+# TUI Agent Delegation Policy (MANDATORY)
+
+You MUST delegate design, analysis, and coding tasks to interactive TUI agents using the `tui_agent` tool. Do NOT write substantive implementation code or architecture designs directly yourself; your role is to orchestrate, delegate, and verify.
+
+## Agent Assignment Rules
+
+1. **Design & Analysis (MANDATORY `codex`)**:
+   - For architecture design, requirements breakdown, codebase reconnaissance, and implementation planning, you MUST call `tui_agent` with `agent: "codex"`.
+2. **Coding & Implementation (MANDATORY `grok` / `agy`)**:
+   - For writing source code, refactoring, bug fixes, and unit tests, you MUST call `tui_agent` with either `agent: "grok"` or `agent: "agy"`.
+   - **50/50 Distribution**: You MUST split coding subtasks evenly (50/50) between `grok` and `agy`.
+3. **Orchestrator Role**:
+   - You (the main agent) coordinate the task: break down goals, invoke `tui_agent`, review results, and run verification/tests (`shell`, `read_file`).
+   - NEVER bypass `tui_agent` to write code directly under the assumption of "avoiding overhead" or "subtask is too small".
+
+## Tool Usage Specification
+
+Always invoke the `tui_agent` tool with both required parameters:
+- `agent`: `"codex"` (design/analysis) | `"grok"` (coding) | `"agy"` (coding)
+- `prompt`: Specific, context-rich instructions for the agent to execute.
+- `session_id` (optional): Reuse an existing session ID if continuing a multi-turn conversation with the same agent.
+
+### Example Tool Invocations:
+
+**Design / Recon:**
+```json
+{
+  "name": "tui_agent",
+  "arguments": {
+    "agent": "codex",
+    "prompt": "Analyze ui/hub/src and design the inline task creation form architecture. Document plan in .agent/task-plan.md."
+  }
+}
+```
+
+**Coding:**
+```json
+{
+  "name": "tui_agent",
+  "arguments": {
+    "agent": "grok",
+    "prompt": "Implement the task draft persistence module in ui/hub/src/taskCreateDraft.ts and write unit tests."
+  }
+}
+```
````

---

## 4. 验证与测试结果

针对 TUI Agent 驱动及调度逻辑，执行了 OpenDuck 相关的全套回归测试：

### 4.1 测试执行命令
```bash
cargo test --test tui_agent_harness_test
```

### 4.2 测试结果输出
```text
running 8 tests
test test_tui_agent_extension_tool_definitions ... ok
test test_harness_prompt_multi_turn_tui_steering ... ok
test test_harness_executes_tui_agent_stop_nonexistent_session ... ok
test test_harness_executes_convenience_tui_agent_missing_params ... ok
test test_harness_executes_tui_agent_list_tool ... ok
test test_harness_handles_tui_agent_start_invalid_agent ... ok
test test_harness_uses_tui_agent_by_prompt_instruction ... ok
test test_harness_prompt_dispatches_between_advisor_and_tui_agent ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 48.09s
```

### 4.3 验证点确认
1. **工具注册一致性** (`test_tui_agent_extension_tool_definitions`): 验证 `tui_agent`, `tui_agent_start`, `tui_agent_prompt`, `tui_agent_read`, `tui_agent_keys`, `tui_agent_list`, `tui_agent_stop` 工具元数据与参数规范全部匹配。
2. **必填参数拦截** (`test_harness_executes_convenience_tui_agent_missing_params`): 校验当缺少 `agent` 或 `prompt` 时能正确截获并报错，印证了规则中补充参数范例的必要性。
3. **指令驱动派发** (`test_harness_uses_tui_agent_by_prompt_instruction` & `test_harness_prompt_dispatches_between_advisor_and_tui_agent`): 验证在指令明确要求使用 TUI Agent 时，Harness Policy 会精准在 Step 0 触发 `tui_agent` 调用。

---

## 5. 后续建议

1. **Task YAML 协同加固**：除全局规则外，在创建复杂的多步骤任务（如 [`.goose/tasks/*.yaml`](file:///mnt/e/openduck-harness/.goose/tasks/)）时，在 Subtask 的 `description` 中一并显式注明委派要求（如“由 TUI Agent Grok 编写 store 模块”），形成双保险。
2. **会话复用（Session Reuse）**：后续在多轮编码中，推荐在 prompt 中提示主 Agent 传递 `session_id`，以保持 Grok / Agy 的上下文记忆。
