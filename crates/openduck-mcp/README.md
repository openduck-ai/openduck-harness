# OpenDuck MCP Extensions (`openduck-mcp`)

Built-in [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) servers and extensions bundled with OpenDuck.

---

## 🔌 Built-in Extensions

### 1. `agy` (Antigravity CLI)
Delegated subagent task execution and Antigravity IDE/CLI session orchestration.
- **In OpenDuck:** `openduck session --with-builtin agy`
- **MCP process:** `openduck mcp agy`
- **User docs:** [Antigravity CLI MCP](../../documentation/docs/mcp/agy-mcp.md)

### 2. `developer` (Developer Tools)
System command execution, file system operations, and directory management.
- **In OpenDuck:** Enabled by default or via `openduck session --with-builtin developer`

### 3. `computercontroller` (`browser_scrape`)
JS-capable browser scraping and snapshot extraction via headless Chrome.
- **In OpenDuck:** `openduck session --with-builtin computercontroller`
- **User docs:** [Computer Controller](../../documentation/docs/mcp/computer-controller-mcp.md)
- **Requires:** Chrome/Chromium on `PATH`.

### 4. `polymarket` (API + Browser)
Market discovery, order book inspection, and trading context.
- **In OpenDuck:** `openduck session --with-builtin polymarket`
- **MCP process:** `openduck mcp polymarket`

---

## 🛠️ Testing with MCP Inspector

Update `examples/mcp.rs` to use the appropriate MCP server:

```bash
npx @modelcontextprotocol/inspector cargo run -p openduck-mcp --example mcp
```
