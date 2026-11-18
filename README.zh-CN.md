<div align="center">

# OpenDuck 🦆

**开源 AI Agent Harness 与企业级智能体系统**

_沙箱执行、基准评测、确定性回放、多项目工作区，以及跨越 CLI、Web Hub、桌面端、移动端和 SDK 的可扩展智能体协议。_

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License: Apache 2.0"></a>
  <a href="https://github.com/openduck-ai/openduck-harness"><img src="https://img.shields.io/badge/Rust-1.94%2B-brightgreen.svg" alt="Rust Edition 2021"></a>
</p>

<p align="center">
  <a href="README.md">English</a> | <strong>简体中文</strong>
</p>
</div>

---

## 🌟 什么是 OpenDuck？

**OpenDuck** 是一个基于 Rust 构建的开源 AI **Agent Harness** 与**企业级智能体系统**，兼具高性能、可移植性与安全性。

OpenDuck 超越了传统的单循环 AI 助手，提供模块化、沙箱化的执行运行时与统一的控制平面。无论你是运行交互式开发会话、执行确定性基准测试、通过可视化 Git 图谱管理多项目工作区，还是调度带邮件告警的无人值守后台任务，OpenDuck 都能为你提供规模化运行、测试和驾驭自主智能体所需的完整基础设施。

CLI 可执行文件为 `openduck`（旧版别名 `goose` 和 `duck` 仍可使用）。工作区 crate 使用 `openduck-*` 前缀。配置和环境变量优先使用 `OPENDUCK_*`。

---

## ✨ 核心特性

### 🧪 1. Agent Harness 与评测运行时
- **解耦的智能体策略**：模块化的 `AgentPolicy` trait 将智能体决策逻辑（ReAct、状态机、外部适配器）与执行运行时清晰解耦。
- **可插拔沙箱（`SandboxDriver`）**：支持在主机隔离环境（`LocalSandbox`、带未跟踪配置符号链接、共享 `CARGO_TARGET_DIR` 及可选自动提交的独立 Git 分支 `WorktreeSandbox`，或 Docker/OCI `ContainerSandbox`）中运行任务，并提供工作区快照、检查点与回滚能力。
- **按项目 FIFO 任务串行化**：每个项目将任务执行入队到专属 FIFO 队列中串行执行，避免对同一 Git 工作副本产生并发修改冲突，同时支持不同项目之间的并行执行。排队中的任务会在 Web Hub 中实时汇报状态。
- **自动任务 Git 提交与变更锁**：任务完成后自动暂存并提交工作区变更，支持可配置的提交信息格式、项目级设置与 UI 开关。通过进程内仓库写锁（`with_repo_write_lock`）保护并发安全，并在进程异常中断时自动恢复残留的 `index.lock`。
- **可插拔 Harness 工具扩展（`HarnessToolExtension`）**：直接为 `AgentHarness` 扩展自定义工具集——包括 `TuiAgentHarnessExtension`（`tui_agent_start`、`tui_agent_prompt`、`tui_agent_read`、`tui_agent_keys`、`tui_agent_stop`、`tui_agent_list`），使 harness 任务能够将工作委派给持久化的交互式编程智能体，支持基于等待的屏幕轮询（`wait_secs`）、未变化屏幕抑制、`stallHint` 检测及有界非阻塞提示词提交；以及用于与工作区任务交互的 `ProjectTasksExtension`。
- **AI 辅助任务起草与内联自动保存创建器**：起草结构化任务规范和按依赖排序的子任务计划，附带验收标准、`ExpectedOutputVerifier` 校验、动态 `max_turns` 同步，以及支持按项目本地持久化草稿的内联自动保存任务创建器。
- **评测与基准测试套件（`openduck harness eval`）**：运行批量基准测试套件（JSONL 数据集、YAML Recipe、SWE-bench 风格任务），支持并行执行、指标评分（通过/失败、轮数、延迟、Token 成本）及轨迹导出。
- **确定性录制与回放（`openduck harness replay`）**：将智能体 LLM 补全与 MCP 工具交互录制为 VCR 风格的 Cassette 文件，用于零成本、可复现的回归测试与离线调试。
- **任务续跑与子任务恢复**：直接从任务历史和回放 Cassette 恢复、重跑或分支后续执行，保留上下文、子任务结果追踪、失败原因处理以及轨迹日志中的实时状态标签。
- **增强型上下文压缩、事实延续与防停滞机制**：压缩后的摘要、通知和重复警告会直接转发至 Provider 上下文（`[HARNESS CONTEXT]`）。`ExtractedContextFacts` 会在多次压缩之间累积并去重已探索的文件、工具观测结果、错误与顾问发现。默认消息预算扩展至 300 条消息，同时完整保留工具调用配对与可见助手文本。`RepetitiveCallDetector` 在出现重复工具调用循环时注入非阻塞警告（在发生变更时重置），并辅以每 15 轮一次的未变更笔记提醒。
- **Provider 遥测、自适应路由与轮次内切换**：追踪 harness 任务执行过程中的 Token 消耗（prompt/completion/cached）、请求延迟、首字延迟（TTFT）、网络阶段耗时、请求量及估算成本，并由具备自动自适应故障转移能力的 **Provider 熔断器（Circuit Breaker）** 提供保障。`adaptive` Provider（`auto`、`regular`、`economy` 模式）可根据非高峰时段与观测到的首字延迟在常规模型与经济模型之间自动路由并定期探测；`AgentRoutingHandle` 支持在传统循环和 `OPENDUCK_STATE_MACHINE=1` 路径下实现实时轮次内模型与 Provider 切换（于下一次推理请求生效）。每次调用的 `routing_tier` 与解析后的模型（`last_actual_candidate`）会通过 `ProviderUsage` 传递至 `TaskProviderStats` 和 `EvaluationReport`（`by_model` 按 `adaptive_tier` 键入）。
- **内存门控与 Jemalloc 堆分析**：进程内内存监控、日志记录与执行前内存门控，防止高负载下的 OOM 故障。支持在 `openduck serve` 中通过 `SIGUSR2` 导出 `jemalloc-prof` 堆采样报告。
- **自动化校验器与安全控制**：内置 `CommandVerifier`（测试运行器输出）、`DiffVerifier`（与预期黄金状态对比的 git patch diff）和 `ExpectedOutputVerifier`（子任务输出校验），同时支持可配置的 `max_turns` 限制（`OPENDUCK_MAX_TURNS`）、防停滞守卫与畸形工具调用的优雅处理。

### 🚀 2. 企业级智能体系统与多项目工作区
- **项目仪表盘**：注册并管理多个软件项目，支持自动目录绑定、元数据标签与洞察摘要。
- **多仓库可视化 Git 管理**：内置 Git 控制模块，提供交互式提交图谱、分支与标签提示、文件差异查看器、Worktree 管理、状态检查、串行化变更锁以及自动提交信息生成。可自动发现项目根目录下的嵌套 Git 仓库、`.gitmodules` 子模块（包括符号链接 Worktree 内的嵌套子模块）、符号链接仓库和 Worktree，并支持将项目级自定义注册仓库合并至发现结果中——提供仓库切换器、按仓库作用域划分的 Git 接口（通过 `repo` / `repo_path` 指定状态、差异、日志、暂存与提交）、上游领先/落后计数以及受进程内写锁（`with_repo_write_lock`）保护的远程同步操作（pull、commit、push）。
- **统一 TUI 智能体管理与配额池**：集中式控制平面与 Web Hub 仪表盘（`/tui-agents`），用于管理智能体配置（`agy`、`claude`、`codex`、`grok`、`custom`）、账号凭据、共享配额池、会话运行绑定及操作审计历史。
- **模型级配额监控、准入控制与 `/quota` 斜杠命令**：跨智能体配置（Grok、Claude、Codex、Agy）的实时配额耗尽与限流检测——包括针对 Agy（`fetchAvailableModels`）和 Codex（会话 JSONL `rate_limits`）的原生配额读取器。按智能体、模型和账号持久化并查询配额快照；在 TUI 启动和发送提示词时执行 fail-open 配额准入检查（仅在存在已知未来重置时间的新鲜耗尽快照时拦截）；支持 Agy 按模型在已发现目录中的启动回退；并提供 `/quota` 和 `/usage` 斜杠命令用于实时查看、刷新、手动重置与暂停。
- **动态 CLI 模型发现与混合选择器**：通过解析 `discovery_command` / `cli_discovery_command` 标准输出动态提取运行时 CLI 模型（Codex、AGY、Grok、Claude），并内置回退目录（Gemini 4 Argon / 3.8、Claude 3.7 / 3.5、GPT-4.5 / o3、Grok 3）、双模式选择器（模糊搜索目录选择 vs. 自定义模型输入）以及通过 REST 与 Web Hub 进行的动态目录刷新。
- **按智能体独立配置的 TUI 出站代理**：通过 REST 与 Web Hub 直接为每个智能体配置独立的 HTTPS 代理设置（`harness.tui_sessions.<agent>.https_proxy`）。
- **项目任务智能体扩展与交互抽屉**：通过专用 UI 抽屉将项目任务查询与操作直接暴露给交互式对话智能体。
- **项目级任务 Harness**：在 `.openduck/tasks/` 中定义、起草和执行可重复任务规范，并通过 `.openduck/harness.yaml` 进行配置。
- **交互式项目终端与 TUI 会话**：直接从各项目工作区管理基于 `tmux` 的持久化智能体 TUI 会话（`agy`、`claude`、`codex`、`grok`、`custom`）。
- **集成 Markdown 与文件浏览器**：直接在 Web Hub 中浏览仓库文件并支持丰富的 Markdown 预览。
- **历史工具参数裁剪**：在 `openduck-context-management` 中自动压缩历史工具调用参数，抑制 Token 膨胀并大幅降低长轨迹中的单步延迟。

### ⏰ 3. Cron 定时任务调度与自动锁
- **进程内 Cron 调度器**：调度周期性 Recipe、健康检查、仓库抓取或基准评测运行。
- **任务并发锁**：防止同一调度任务重叠执行，并提供自动状态追踪（`running`、`completed`、`failed`、`skipped`）。
- **无头 Recipe 自动化**：无人值守执行多步骤结构化 Recipe，并完整持久化会话记录。

### 📬 4. 通知与报告系统
- **SMTP 邮件通知**：在任务完成、执行失败或定时报告生成时自动发送告警邮件。
- **可配置任务报告与 PDF 导出**：包含任务状态、耗时、Diff 与校验指标的 HTML/Markdown 格式邮件摘要，以及支持完整 CJK 字符集的任务轨迹与结论独立 PDF 导出。

### 🔌 5. 广泛的模型上下文与协议生态
- **15+ LLM Provider 与推理模型**：支持 Anthropic、OpenAI、Google Gemini（含思考签名提取）、Ollama（本地模型）、OpenRouter、Azure Foundry、AWS Bedrock、Databricks、Snowflake 以及 Qwen（支持 Qwen 3.8 Flash、QwQ 及思考变体的推理强度映射）。
- **解耦的 Provider/模型选择与自适应路由**：独立于 Provider 定义的角色模型解析（`active_model`、`secondary_provider_model`），支持通过优先级层级让主角色和辅助角色共享同一 Provider 但使用各自独立的模型（V2 配置 API `selectionVersion: 2`）。`adaptive` Provider（`auto`、`regular`、`economy` 模式）利用非高峰时段、观测到的 TTFT 延迟与定期探测在常规和经济模型之间路由；`AgentRoutingHandle` 支持在下一次推理请求时实时完成轮次内切换。
- **全局出站 HTTP 与 SOCKS 代理支持**：通过声明式配置文件（`config.yaml`）、环境变量（`HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`）或 Web Hub UI 配置 HTTP 或 SOCKS5 代理以连接 Provider 与 MCP 服务器，内置凭据脱敏、网络健康监控与端点可达性诊断。
- **模型上下文协议（MCP）**：原生支持 70+ 社区 MCP 扩展及以下内置扩展：
  - `tui_agent` / `tui_session`：持久化交互式编程智能体 TUI 与 tmux 驱动（`agy`、`grok`、`codex`、`claude`、`custom`），支持括号粘贴（bracketed paste）、空闲提示符检测（含带边框与无边框输入框）、旋转指示器与盲文字符画区分、交互式弹窗/按键应答、基于等待的 `tui_agent_read`（`wait_secs`、未变化屏幕抑制、`stallHint`）、带忙碌会话拒绝的 10 秒有界提示词提交、无损旁路转录关联、人工实时接入、动态 CLI 模型发现、模型级配额追踪与准入控制，以及原生 `AgentHarness` 工具集成。
  - `agy`：用于委派子智能体任务执行的 Antigravity CLI 集成。
  - `developer`：本地 Shell 命令执行、文件读取与编辑。
  - `computercontroller`：无头浏览器抓取与自动化网页交互。
  - `polymarket`、`memory`、`tutorial` 等更多扩展。
- **Agent Client Protocol (ACP)**：基于 HTTP/SSE、WebSocket 与 stdio 的标准化流式协议，用于丰富的 UI 客户端集成。

### 🖥️ 6. 统一的多端界面套件
- **OpenDuck CLI**：功能完备的终端界面（`session`、`run`、`harness`、`serve`、`schedule`、`term`、`doctor`），内置交互式斜杠命令，包括用于智能体配额检查与管理的 `/quota` 和 `/usage`。
- **OpenDuck Web Hub（`ui/hub`）**：企业级 Web 控制台，包含：
  - **完整国际化（i18n）**：支持英文与中文（EN / ZH）语言及实时语言切换器。
  - **任务与运行中任务页面（`/running-tasks`）**：统一展示活跃的 harness 作业与历史任务运行记录，支持状态筛选、分页、实时阶段、当前工具调用、截断参数、耗时计时器、取消控制、内联轨迹/日志检查，以及直达项目对话和日志的跳转。
  - **TUI 智能体页面（`/tui-agents`）**：统一管理智能体配置、账号绑定、配额池、活跃会话与审计历史，配备动态 CLI 模型发现、双模式模型选择器（模糊搜索目录与自定义模型自由输入）以及带可用性/停滞提示和手动重置控制的模型级配额状态徽章。
  - **内联自动保存任务创建器**：按项目划分的可折叠自动保存草稿编辑器，支持本地草稿持久化与丢弃控制。
  - **PDF 轨迹与结论导出**：将完整执行轨迹与结论导出为带样式的 PDF 进行下载、打印或预览，支持 CJK 字体（STSong-Light）。
  - **深层链接与历史导航**：任务历史、智能体日志、文件预览与分栏状态的 URL 查询参数同步。
  - **设置分组导航与自适应路由**：服务端设置中带平滑滚动追踪的本地化侧边栏导航，包含自适应路由配置（`auto`/`regular`/`economy` 模式、非高峰时段）以及模型选择器中的实时路由标签。
  - **多仓库 Git 与项目工作区**：自动发现项目目录中的嵌套 Git 仓库、`.gitmodules` 子模块（含符号链接 Worktree 内的嵌套子模块）、符号链接仓库和 Worktree，支持项目级自定义仓库注册、仓库选择下拉框、按仓库作用域划分的状态/差异/暂存/提交工作流、上游领先/落后提交计数、工具栏与行级远程操作（pull、push 及子仓库提交，含冲突保护）、提交图谱与提交信息生成器。
- **OpenDuck Desktop（`ui/desktop`）**：面向 macOS、Windows 和 Linux 的原生 Electron 桌面应用。
- **OpenDuck Mobile PWA（`ui/mobile`）**：随时随地操控智能体并审批权限的移动端 Web 客户端。
- **跨语言 SDK**：为 TypeScript（`@openduck/sdk`）、Python、Rust、JVM/Kotlin（`io.github.aaif-goose:gdk`）和 Swift 提供一等客户端库。

---

## 🏗️ 架构概览

```
+-----------------------------------------------------------------------------------+
|                                OPENDUCK SYSTEM                                    |
+-----------------------------------------------------------------------------------+
|  [ 接口与前端 (Interfaces & Frontends) ]                                          |
|   ├── OpenDuck CLI (`openduck`, `goose`, `duck`)                                  |
|   ├── OpenDuck Web Hub (`ui/hub` & `ui/hub-core` React 控制台, i18n EN/ZH)         |
|   ├── OpenDuck Desktop (`ui/desktop` Electron 桌面应用)                           |
|   ├── OpenDuck Mobile PWA (`ui/mobile` 移动端 PWA)                                |
|   └── 跨语言 SDK (TypeScript `@openduck/sdk`, Python, Kotlin, Rust)              |
+-----------------------------------------------------------------------------------+
|  [ 控制平面与服务端 (`openduck serve` / ACP & REST) ]                              |
|   ├── 项目工作区注册表、TUI 会话管理器与模型级配额存储                             |
|   ├── 按项目 FIFO 任务队列与串行化 Git 变更锁                                      |
|   ├── 多仓库 Git 引擎（子模块、符号链接、Worktree、自定义仓库）                     |
|   ├── 浏览器内配置管理、出站代理、自适应与解耦路由                                 |
|   ├── 任务调度器（Cron、任务锁、并发控制）                                         |
|   ├── SMTP 告警与 PDF 轨迹导出器（支持 CJK 的 STSong-Light）                       |
|   ├── 内存监控、OOM 门控与堆分析（`jemalloc-prof`）                                |
|   └── 认证与多会话数据库（SQLite `sessions.db`, `control.db`）                     |
+-----------------------------------------------------------------------------------+
|  [ Agent Harness 运行时 (`openduck-harness`) ]                                    |
|   ├── 可插拔智能体策略（`AgentPolicy`: ReAct、状态机、适配器）                    |
|   ├── 实时轮次内路由（`AgentRoutingHandle`, `ProviderRoutingStatus`）              |
|   ├── 沙箱驱动（`SandboxDriver`: Local、Git Worktree、Docker Container）           |
|   ├── 可插拔工具扩展（`HarnessToolExtension`: TuiAgent, ProjectTasks）             |
|   ├── 任务起草、内联自动保存创建器与评测引擎（`EvalRunner`）                       |
|   ├── 任务续跑、子任务恢复与轨迹状态追踪                                           |
|   ├── Provider 遥测、路由层级与轨迹指标（Tokens, TTFT）                            |
|   └── 确定性录制与回放层（`Cassette`, `MockProvider`）                             |
+-----------------------------------------------------------------------------------+
|  [ 上下文与优化层 (`openduck-context-management`) ]                               |
|   ├── 事实延续（`ExtractedContextFacts`, `[HARNESS CONTEXT]`）                     |
|   ├── 防停滞循环保护（`RepetitiveCallDetector`）                                   |
|   └── 上下文压缩、剪枝与历史工具调用参数裁剪                                       |
+-----------------------------------------------------------------------------------+
|  [ 工具与扩展层 (Tool & Extensibility Layer) ]                                    |
|   ├── 模型上下文协议（MCP）服务器（tui_session, agy, dev, browser 等）             |
|   ├── 动态 CLI 模型发现与实时目录切换（Codex, Grok, Claude）                       |
|   └── 15+ LLM Provider（OpenAI, Anthropic, Gemini, Ollama, Qwen, Azure, 代理等）   |
+-----------------------------------------------------------------------------------+
```

---

## 🚀 快速开始

> 🤖 **AI Agent 与无头自动化环境**：请参阅非交互式、带校验步骤的 [AI Agent 安装与配置执行手册](documentation/docs/guides/agent-setup-guide.zh-CN.md)。

### 1. 安装 CLI

使用官方安装脚本安装预编译二进制文件：

```bash
curl -fsSL https://github.com/openduck-ai/openduck-harness/releases/download/stable/download_cli.sh | bash
```

_该脚本会安装 `openduck` 以及兼容别名 `goose` 和 `duck`。_

或使用 Rust 从源码构建：

```bash
git clone https://github.com/openduck-ai/openduck-harness
cd openduck-harness
source bin/activate-hermit   # 激活锁定的 Rust、Node、pnpm 工具链
cargo build --release
```

### 2. 配置 Provider

运行交互式配置向导：

```bash
openduck configure
```

或直接设置 Provider 环境变量：

```bash
# 示例：Anthropic Claude
export ANTHROPIC_API_KEY="your-api-key"
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"

# 示例：OpenAI GPT
export OPENAI_API_KEY="your-api-key"
export OPENDUCK_PROVIDER__TYPE="openai"
export OPENDUCK_PROVIDER__MODEL="gpt-4o"

# 示例：本地 Ollama
export OPENDUCK_PROVIDER__TYPE="ollama"
export OPENDUCK_PROVIDER__HOST="http://localhost:11434"
export OPENDUCK_PROVIDER__MODEL="qwen2.5-coder:latest"
```

验证环境配置：

```bash
openduck doctor
openduck info --check
```

### 3. 后台守护进程与 Web Hub

启动 OpenDuck ACP + REST 后台服务：

```bash
# 启动后台服务（例如端口 3000 或默认 3284）
export OPENDUCK_SERVER__SECRET_KEY="your-strong-secret-key"
openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler
```

启动 OpenDuck Web Hub 前端控制台：

```bash
# 在另一个终端中启动 Web Hub UI
cd ui/hub
pnpm install
pnpm run dev
```

在浏览器中打开 [http://localhost:5173/hub/](http://localhost:5173/hub/) 并使用你配置的 Secret Key 连接。

---

## 📖 CLI 使用示例

### 交互式对话会话
```bash
# 启动新的交互式会话
openduck session

# 恢复最近的会话，或按 ID/名称恢复指定会话
openduck session --resume
openduck session --resume --name "auth-refactor"

# 启用内置或自定义 MCP 扩展
openduck session --with-builtin developer,agy,tui_agent
```

### 🖥️ 持久化智能体 TUI 与 Tmux 会话
以零冷启动、多行括号粘贴、动态模型切换与实时终端接入的方式交互式驱动外部编程智能体：

```bash
# 启动启用了 tui_agent 驱动的 OpenDuck 会话
openduck session --with-builtin tui_agent

# 实时接入运行中的智能体会话（agy、grok、codex 或 claude）
tmux -L openduck attach -t <session_id>

# 通过 REST 发现模型目录或切换运行中会话的模型
curl http://localhost:3000/api/tui/agents/claude/models
curl -X POST http://localhost:3000/api/tui/sessions/<session_id>/switch-model \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-3-7-sonnet"}'

# 从智能体 CLI 帮助/发现输出中动态解析可用模型
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/parse-models

# 直接向活跃的 TUI 智能体会话发送提示词或原始按键
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt":"Refactor auth tests"}'
curl -X POST http://localhost:3000/api/v1/tui-agents/<agent_id>/keys \
  -H "Content-Type: application/json" \
  -d '{"keys":"y\n"}'
```

### 📊 智能体配额管理与斜杠命令
在对话会话中交互式监控智能体用量限制并解除已耗尽配额的拦截：

```markdown
- `/quota` 或 `/usage`         # 显示智能体配额、剩余百分比与重置倒计时表格
- `/quota refresh [agent]`     # 从 Provider/日志重新查询配额状态
- `/quota reset <agent>`       # 将智能体配额重置为 100%（有效）并立即解除拦截
- `/quota set <agent> <pct>`   # 手动设置剩余配额百分比（例如 `/quota set grok 80%`）
- `/quota block <agent>`       # 手动将配额标记为已耗尽并暂停智能体直至重置
```

### 无头指令执行
```bash
# 从提示词或 Markdown 指令文件执行任务
openduck run "Audit this repository for security vulnerabilities"
openduck run --file instructions.md
```

### 🧪 Agent Harness：评测、基准测试与回放
```bash
# 1. 在任务数据集上运行批量基准评测
openduck harness eval --dataset benchmarks/swe_tasks.jsonl --concurrency 4 --output-dir eval_results/

# 2. 执行单个任务并将执行过程录制到 Cassette 文件
openduck harness run --prompt "Fix broken test in tests/auth.rs" --record cassettes/auth_fix.json

# 3. 在独立分支的隔离 Git Worktree 中执行任务并自动提交
openduck harness run --prompt "Refactor error handling in git module" --worktree --worktree-branch "feature/git-errors" --auto-commit

# 4. 离线确定性回放已录制的 Cassette（零 API 成本）
openduck harness replay --cassette cassettes/auth_fix.json
```

### Cron 定时任务调度
```bash
# 添加定时 Recipe 作业
openduck schedule add --name "nightly-audit" --cron "0 2 * * *" --recipe recipes/security_scan.yaml

# 列出定时作业并检查状态
openduck schedule list
openduck schedule run-now --id <JOB_ID>
```

---

## 🌐 OpenDuck Web Hub 与前端

### OpenDuck Web Hub（`ui/hub`）
Web Hub 是一个连接到 `openduck serve` 的企业级智能体管理控制台：
- **完整国际化（i18n）**：原生支持英文与中文（EN / ZH）及持久化语言切换器。
- **项目工作区**：注册、切换并检查多个软件仓库。
- **多仓库 Git 与 Worktree 可视化工具**：自动发现项目目录中的嵌套 Git 仓库、`.gitmodules` 子模块（含符号链接 Worktree 内的嵌套子模块）、符号链接仓库和 Worktree，支持项目级自定义仓库注册；提供仓库切换下拉框、按仓库作用域划分的状态/差异/暂存/提交工作流、上游领先/落后提交计数、针对所选仓库的工具栏拉取/推送以及受冲突保护与写锁同步保护的统一行操作（pull、push 与子仓库提交）、交互式提交日志、分支头、标签和自动提交信息生成。
- **Harness 任务运行器、AI 起草与续跑**：支持本地草稿持久化的内联自动保存任务创建器、带同步 `max_turns` 预算的按依赖排序子任务、带可选自动提交的隔离 Git Worktree、历史运行的后续追问以及 Cassette 回放。
- **统一运行中与历史任务页面（`/running-tasks`）**：跨项目统一展示活跃 harness 作业与历史任务运行的实时监控页面，支持状态筛选、分页、当前执行阶段、活跃工具调用、截断参数、耗时计时器、取消控制、内联轨迹/日志检查以及直达项目对话和日志的跳转。
- **统一 TUI 智能体控制台（`/tui-agents`）**：管理编程智能体配置（`agy`、`claude`、`codex`、`grok`、`custom`）、账号凭据绑定、配额池、活跃会话及操作审计历史。包含动态 CLI 模型发现（“Discover Models from CLI”）、双模式模型选择器（模糊搜索目录选择 vs. 自定义模型自由输入）以及带可用性/停滞提示和手动重置控制的模型级配额状态徽章。
- **交互式项目终端与 TUI 会话**：启动、监控并接入基于 `tmux` 的持久化编程智能体会话，通过模糊搜索动态切换活跃模型。
- **Provider 统计与网络耗时面板**：实时监控跨 Provider、模型和自适应路由层级（`adaptive_tier` 徽章）的 Token 消耗、延迟、TTFT、网络阶段耗时、请求量与成本。
- **分页轨迹与编程智能体转录视图**：服务端分页轨迹查看器（`TaskTrajectoryViewer`）与 `AgentLogTranscript`，支持可配置分页大小、可折叠的推理/思考块、自适应路由层级徽章、解析后的 TUI 会话状态卡片与实时状态指示器。
- **PDF 轨迹与结论导出**：将完整执行轨迹与结论卡片导出为支持 STSong-Light UTF-16BE CJK 字体的 PDF，用于直接下载、打印或预览。
- **浏览器内配置与网络诊断**：直接在浏览器中查看并编辑 OpenDuck 设置、解耦的 Provider/角色模型、自适应模型路由（`auto`/`regular`/`economy` 模式与非高峰时段）及代理配置（`ServerSettings` 与 `NetworkStatusPanel`），提供本地化分组导航、活跃环境变量覆盖指示器以及实时连通性诊断。
- **URL 导航与深层链接**：状态与 URL 查询参数（`page`、`project`、`tab`、`split`）保持同步，完整支持任务历史、智能体日志与文件预览的浏览器历史导航。
- **对话与会话管理器**：流式接收带工具审批的实时 ACP 响应、项目任务对话抽屉以及动态 Provider/模型切换。
- **定时调度与邮件告警**：配置自动化 Cron 作业与 SMTP 通知收件人。

在开发模式下启动 Web Hub：
```bash
cd ui/hub
pnpm install
pnpm run dev
```

### OpenDuck Desktop（`ui/desktop`）
面向 macOS、Windows 和 Linux 的跨平台 Electron 桌面应用。
```bash
cd ui/desktop
pnpm install
pnpm run start
```

---

## 📦 工作区仓库结构

```
crates/
├── openduck                 # 核心智能体逻辑、控制平面、调度器、Git、通知
├── openduck-harness         # Harness 运行时、沙箱驱动、评测引擎、回放 Cassette
├── openduck-agent           # 可插拔智能体策略与状态机
├── openduck-cli             # CLI 入口（`openduck`, `goose`, `duck`）
├── openduck-mcp             # MCP 扩展（tui_session, agy, developer, computer controller, polymarket, memory）
├── openduck-providers       # LLM Provider 实现（Anthropic, OpenAI, Gemini, Ollama 等）
├── openduck-provider-types  # 规范化 Provider 请求/响应类型定义
├── openduck-sdk             # 跨语言 SDK 绑定与 UniFFI（Python, Kotlin/Maven, Swift）
├── openduck-context-management # 上下文剪枝、提示词压缩与记忆摘要
├── openduck-download-manager   # 二进制与模型资产下载管理
├── openduck-local-inference    # 基于 Candle 的本地模型推理执行
├── openduck-test            # 集成与端到端测试套件
└── openduck-test-support    # Mock 服务器、测试夹具与断言工具

ui/
├── hub                      # OpenDuck Web Hub（React 19 + Vite 控制台）
├── hub-core                 # Web Hub 共享 REST 与 ACP 客户端逻辑
├── desktop                  # Electron 桌面应用
├── mobile                   # 移动端 PWA 客户端
├── sdk                      # TypeScript SDK 包（@openduck/sdk）
└── text                     # 终端 UI（TUI）包
```

---

## ⚙️ 配置与环境变量

OpenDuck 遵循标准 XDG 目录规范存储配置：
- **配置根目录**：`~/.config/openduck/`（旧版回退：`~/.config/goose/`）
- **数据与会话**：`~/.local/share/openduck/`（旧版回退：`~/.local/share/goose/`）
- **状态与运行**：`~/.local/state/openduck/`（旧版回退：`~/.local/state/goose/`）

有关 `~/.config/openduck/` 中所有配置文件（包括 `config.yaml`、`secrets.yaml`、`notifications.yaml`、`permission.yaml`、全局 `proxy` 及自定义 Provider）的详细说明，请参阅 [Configuration Files Guide](documentation/docs/guides/config-files.md)。关于运行和调度任务，请参阅 [Running Tasks Guide](documentation/docs/guides/running-tasks.md)。关于隔离 Git Worktree 任务执行，请参阅 [Git Worktree Task Runner Guide](documentation/docs/guides/git-worktree-task-runner.md)。关于持久化智能体 CLI TUI 会话、配额与代理配置，请参阅 [Interactive TUI Sessions Guide](documentation/docs/guides/tui-sessions.md) 和 [TUI Driver Extension Reference](documentation/docs/mcp/tui-agent-mcp.md)。关于服务端内存诊断与堆分析，请参阅 [Jemalloc Heap Profiles Guide](documentation/docs/guides/jemalloc-prof.md)。

### 核心环境变量

| 变量 | 说明 |
|---|---|
| `OPENDUCK_PROVIDER__TYPE` | 活跃的 LLM Provider（例如 `anthropic`、`openai`、`google`、`ollama`） |
| `OPENDUCK_PROVIDER__MODEL` | Provider 默认模型标识符（例如 `claude-3-7-sonnet-latest`、`gpt-4o`） |
| `OPENDUCK_ACTIVE_MODEL` | 主智能体角色的解耦模型覆盖（优先级高于 Provider 默认值） |
| `OPENDUCK_SECONDARY_MODEL` | 辅助/评测智能体角色的解耦模型覆盖 |
| `OPENDUCK_PROXY` | 用于 Provider 与 MCP 网络请求的出站 HTTP/SOCKS 代理 URL |
| `OPENDUCK_MAX_TURNS` | Harness 与任务执行的最大轮次限制（别名：`GOOSE_MAX_TURNS`、`HARNESS_MAX_TURNS`） |
| `OPENDUCK_TASK_TIMEOUT` | 单任务执行超时时间（秒） |
| `OPENDUCK_MEMORY_LIMIT_MB` | 触发执行前内存门控的内存阈值上限 |
| `OPENDUCK_SERVER__SECRET_KEY` | 用于认证 ACP 与 REST API 请求的共享密钥 |
| `OPENDUCK_STATE_MACHINE` | 设为 `1` 以启用下一代智能体状态机 |
| `OPENDUCK_DISABLE_KEYRING` | 设为 `1` 以将密钥存储在 `~/.config/openduck/secrets.yaml` 而非操作系统密钥环 |
| `OPENDUCK_NOTIFICATION_CONFIG` | 覆盖 `notifications.yaml` 的自定义文件路径 |
| `OPENDUCK_PATH_ROOT` | 覆盖所有 XDG 配置与数据路径的绝对根目录 |
| `OPENDUCK_LOG_LEVEL` | Tracing 日志级别（`info`、`debug`、`trace`） |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` | 用于路由 Provider 与 MCP 网络请求的标准 HTTP/SOCKS 代理 URL |

_注：所有 `GOOSE_*` 环境变量与配置路径均保留作为向后兼容别名。_
