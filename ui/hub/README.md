# OpenDuck Web Hub (`ui/hub`)

**OpenDuck Web Hub** is a personal agent management dashboard and web interface for OpenDuck. It connects directly to an authenticated background `openduck serve` instance over REST and the Agent Client Protocol (ACP).

---

## 🚀 Features

- 📁 **Project Dashboard**: Register and manage multiple codebases with directory bindings, tags, status tracking, and insight summaries.
- 🌳 **Interactive Git Graph**: Visual commit log graph with branch heads, tag labels, status badges, diff inspection, and automated AI commit message generation.
- 🧪 **Agent Harness & Evaluation Hub**:
  - Run and monitor benchmark evaluation suites.
  - Execute project harness tasks defined in `.goose/tasks/` or `.openduck/tasks/`.
  - Inspect execution trajectories, turn step logs, and replay recorded cassettes.
- 💬 **Live Chat & ACP Streaming**: Full session management with live token streaming, tool call visualization, and interactive permission approvals.
- ⏰ **Job Scheduler**: Manage recurring cron jobs with task locking to prevent overlapping runs.
- 📬 **Notification & Alert Settings**: Configure SMTP email notifications, test SMTP connections, and customize task reporting templates.
- 🌗 **Light / Dark Mode**: Full theme customization with system preference detection and persistence.

---

## 🛠️ Development & Running

### Prerequisites
1. Ensure Hermit is active:
   ```bash
   source bin/activate-hermit
   ```
2. Start the OpenDuck background daemon:
   ```bash
   export OPENDUCK_SERVER__SECRET_KEY="my-secret-key"
   openduck serve --host 127.0.0.1 --port 3284 --enable-scheduler
   ```

### Running the Web Hub
```bash
cd ui/hub
pnpm install
pnpm run dev
```

Open `http://localhost:5173` in your browser and connect to your `openduck serve` endpoint (default: `http://127.0.0.1:3284`) using your configured secret key.

---

## 🧪 Testing

```bash
cd ui/hub
pnpm test
pnpm run typecheck
```
