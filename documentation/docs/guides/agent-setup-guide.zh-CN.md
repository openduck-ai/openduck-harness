---
sidebar_position: 3
title: AI Agent 安装与配置指南
sidebar_label: AI Agent 安装指南 (中文)
---

# OpenDuck AI Agent 安装与配置执行手册

> **适用对象**：自主编程智能体（AI Agent）、CI Runner 及无头自动化环境。  
> **编写原则**：所有步骤均为非交互式、确定性命令，并在每步末尾附带显式校验命令。在无人工介入的环境中，请勿运行交互式配置向导（`openduck configure`）。

---

## 执行前置约定（必读）

- **工作目录**：所有 `cargo` 和 `bin/activate-hermit` 命令均假定当前目录为 `openduck-harness/` 仓库根目录。
- **工具链激活**：每个新开启的 Shell 会话在执行 `cargo`、`node` 或 `pnpm` 前，必须先运行 `source bin/activate-hermit`。
- **无头密钥存储**：在非交互式 Shell 中务必设置 `export OPENDUCK_DISABLE_KEYRING=1`，避免 OpenDuck 因等待系统密钥环弹窗而阻塞。
- **端口匹配**：`ui/hub` 开发服务器（`vite.config.ts`）默认将 `/api`、` /acp`、`/hub/api` 和 `/hub/acp` 代理到 `http://127.0.0.1:3000`。启动 `ui/hub` 时，`openduck serve` 必须指定 `--port 3000`。

---

## 第 1 步：克隆 Git 仓库

```bash
git clone https://github.com/openduck-ai/openduck-harness.git
cd openduck-harness
```

**校验**：
```bash
test -f Cargo.toml && test -f bin/activate-hermit && echo "OK: repo ready"
```

---

## 第 2 步：激活工具链并编译

激活 Hermit 锁定的工具链（Rust、Node.js、pnpm），然后编译 Rust 工作区：

```bash
source bin/activate-hermit
cargo build --release
```

> 若在开发期需要更快编译速度，可使用 `cargo build`（产物路径为 `target/debug/openduck`）。

将编译产物加入当前 Shell 的 `PATH`：

```bash
export PATH="$PWD/target/release:$PATH"
```

安装前端工作区依赖（`ui/hub` 所需）：

```bash
cd ui
pnpm install
cd ..
```

**校验**：
```bash
openduck --version
pnpm --version
```

---

## 第 3 步：配置 LLM Provider（非交互式）

请**不要**运行 `openduck configure`（该命令为交互式 TUI 向导）。推荐使用**方式 A**（环境变量，适合临时会话或 CI）或**方式 B**（声明式 YAML 配置文件，适合持久化环境）。

在无头环境中，始终禁用系统密钥环：

```bash
export OPENDUCK_DISABLE_KEYRING=1
```

### 方式 A：通过环境变量配置（最快）

任选**一组** Provider 配置并导出到当前 Shell：

```bash
# 选项 1：Anthropic
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"
export ANTHROPIC_API_KEY="sk-ant-..."

# 选项 2：OpenAI
export OPENDUCK_PROVIDER__TYPE="openai"
export OPENDUCK_PROVIDER__MODEL="gpt-4o"
export OPENAI_API_KEY="sk-..."

# 选项 3：Google Gemini
export OPENDUCK_PROVIDER__TYPE="google"
export OPENDUCK_PROVIDER__MODEL="gemini-2.5-pro"
export GEMINI_API_KEY="AIza..."

# 选项 4：本地 Ollama（无需 API Key）
export OPENDUCK_PROVIDER__TYPE="ollama"
export OPENDUCK_PROVIDER__HOST="http://127.0.0.1:11434"
export OPENDUCK_PROVIDER__MODEL="qwen2.5-coder:latest"
```

可选出站代理（若网络环境需要）：

```bash
export OPENDUCK_PROXY="http://127.0.0.1:3128"
```

### 方式 B：通过配置文件（`~/.config/openduck/`）

创建 `~/.config/openduck/config.yaml` 和 `~/.config/openduck/secrets.yaml`：

```bash
mkdir -p ~/.config/openduck

cat <<'EOF' > ~/.config/openduck/config.yaml
active_provider: anthropic
providers:
  anthropic:
    enabled: true
    configured: true
    model: "claude-3-7-sonnet-latest"
GOOSE_MODE: auto
EOF

cat <<'EOF' > ~/.config/openduck/secrets.yaml
ANTHROPIC_API_KEY: "sk-ant-..."
EOF
chmod 600 ~/.config/openduck/secrets.yaml
```

#### 自定义 Provider（`~/.config/openduck/custom_providers/`）

如需接入自托管或第三方兼容端点（如 vLLM、LiteLLM、Unsloth 或私有网关）而无需修改源码，可在 `~/.config/openduck/custom_providers/` 目录下放置 `*.json` 定义文件。`engine` 支持 `openai`、`anthropic` 和 `ollama`（以及对应的 `*_compatible` 别名）。

```bash
mkdir -p ~/.config/openduck/custom_providers

cat <<'EOF' > ~/.config/openduck/custom_providers/vllm_local.json
{
  "name": "vllm_local",
  "engine": "openai",
  "display_name": "vLLM Local",
  "description": "Self-hosted OpenAI-compatible inference server",
  "base_url": "http://127.0.0.1:8000/v1/chat/completions",
  "api_key_env": "VLLM_API_KEY",
  "requires_auth": true,
  "supports_streaming": true,
  "models": [
    {
      "name": "Qwen/Qwen2.5-Coder-32B-Instruct"
    }
  ]
}
EOF
```

随后在 `~/.config/openduck/config.yaml` 中使用该 Provider 的 `name` 激活配置，并在 `~/.config/openduck/secrets.yaml` 中写入对应的密钥（当 `requires_auth` 为 `true` 时）：

```bash
cat <<'EOF' > ~/.config/openduck/config.yaml
active_provider: vllm_local
providers:
  vllm_local:
    enabled: true
    configured: true
    model: "Qwen/Qwen2.5-Coder-32B-Instruct"
GOOSE_MODE: auto
EOF

cat <<'EOF' > ~/.config/openduck/secrets.yaml
VLLM_API_KEY: "sk-local-..."
EOF
chmod 600 ~/.config/openduck/secrets.yaml
```

完整字段说明（如 `headers`、`timeout_seconds`、`env_vars`、`dynamic_models`、`proxy` 等）请参阅 [config-files.md](./config-files.md#7-custom_providersjson--declarative-custom-providers)。

**校验**：
```bash
openduck doctor
openduck info --check
```

---

## 第 4 步：启动并验证 Agent Harness

### 4.1 零成本冒烟测试

使用内置 `EchoPolicy` 验证 Harness 运行时是否正常（不调用真实 LLM API）：

```bash
openduck harness run --echo --prompt "smoke test"
```

### 4.2 运行 Harness CLI 任务

```bash
# 单任务执行
openduck harness run --prompt "Inspect repository structure and summarize"

# 在隔离的 Git Worktree 分支中执行并自动提交
openduck harness run \
  --prompt "Fix failing unit test" \
  --worktree \
  --worktree-branch "agent/fix-test" \
  --auto-commit

# 录制执行过程为 Cassette，并离线确定性回放
openduck harness run --prompt "Summarize README.md" --record cassettes/smoke.json
openduck harness replay --cassette cassettes/smoke.json

# 批量基准评测
openduck harness eval \
  --dataset benchmarks/swe_tasks.jsonl \
  --concurrency 4 \
  --output-dir harness_eval_results/
```

### 4.3 启动 Harness 控制平面服务（`openduck serve`）

控制平面提供 `ui/hub`、任务队列与定时调度所需的 ACP + REST API。请指定端口 `3000`（与 `ui/hub/vite.config.ts` 代理配置一致）：

```bash
export OPENDUCK_DISABLE_KEYRING=1
export OPENDUCK_SERVER__SECRET_KEY="replace-with-strong-secret"

openduck serve \
  --platform desktop \
  --host 0.0.0.0 \
  --port 3000 \
  --enable-scheduler
```

> 若需在后台自动化运行，可将日志重定向并在后台启动：  
> `nohup openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler > /tmp/openduck-serve.log 2>&1 &`

**校验**：
```bash
curl -fsS http://127.0.0.1:3000/health
```

---

## 第 5 步：启动 `ui/hub`（Web Hub 控制台）

确保 `openduck serve` 已在 `3000` 端口监听（见第 4.3 步）。

### 开发模式（支持热更新）

```bash
source bin/activate-hermit
cd ui/hub
pnpm install
pnpm run dev
```

> 后台运行方式：  
> `nohup pnpm run dev > /tmp/openduck-hub.log 2>&1 &`

**校验**：
```bash
curl -fsS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5173/hub/
# 预期输出: 200
```

访问地址：`http://localhost:5173/hub/`  
在页面中使用第 4.3 步配置的 `OPENDUCK_SERVER__SECRET_KEY` 进行认证连接。

### 生产构建（可选）

```bash
source bin/activate-hermit
cd ui/hub
pnpm run build
```

构建产物输出至 `ui/hub/dist/`。

---

## 一键快速执行序列（可直接复制执行）

```bash
# 1. 克隆并激活工具链
git clone https://github.com/openduck-ai/openduck-harness.git
cd openduck-harness
source bin/activate-hermit

# 2. 编译二进制并安装前端依赖
cargo build --release
export PATH="$PWD/target/release:$PATH"
(cd ui && pnpm install)

# 3. 无头配置 Provider
export OPENDUCK_DISABLE_KEYRING=1
export OPENDUCK_PROVIDER__TYPE="anthropic"
export OPENDUCK_PROVIDER__MODEL="claude-3-7-sonnet-latest"
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENDUCK_SERVER__SECRET_KEY="replace-with-strong-secret"

# 4. 环境校验、Harness 冒烟测试并启动后台服务
openduck doctor
openduck harness run --echo --prompt "smoke test"
nohup openduck serve --platform desktop --host 0.0.0.0 --port 3000 --enable-scheduler > /tmp/openduck-serve.log 2>&1 &
sleep 2 && curl -fsS http://127.0.0.1:3000/health

# 5. 启动 ui/hub
cd ui/hub
nohup pnpm run dev > /tmp/openduck-hub.log 2>&1 &
sleep 2 && curl -fsS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5173/hub/
```

---

## AI Agent 常见故障排查

| 现象 | 原因 | 解决方法 |
|---|---|---|
| 找不到 `cargo` 或 `pnpm` | 当前 Shell 未激活 Hermit 工具链 | 在仓库根目录执行 `source bin/activate-hermit` |
| CLI 在读取 Provider/密钥时卡死 | 无头环境下等待系统密钥环弹窗 | 设置 `export OPENDUCK_DISABLE_KEYRING=1` |
| `openduck serve` 启动失败 | 未设置 `OPENDUCK_SERVER__SECRET_KEY` | 导出 `OPENDUCK_SERVER__SECRET_KEY`（仅本地测试可加 `--dangerously-unauthenticated`） |
| `ui/hub` 请求接口报 `502` / `ECONNREFUSED` | `openduck serve` 未监听在 `3000` 端口 | 重启 `openduck serve` 并显式传入 `--port 3000`（CLI 默认端口为 `3284`） |
| `pnpm run dev` 找不到 workspace 包 | 未在 `ui/` 根目录安装工作区依赖 | 执行 `cd ui && pnpm install` |
