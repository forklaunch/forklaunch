---
'@forklaunch/core': minor
'@forklaunch/express': minor
'@forklaunch/hyper-express': minor
---

Serve MCP at `/mcp` (or `mcp.path`) on the application port, the way Rust services do, instead of a second listener on `MCP_PORT` / `PORT + 2000`. A separate listener is still used when `mcp.port`, `MCP_PORT` or `MCP_LISTENER=separate` is set; set `MCP_LISTENER=separate` to keep MCP clients that point at `PORT + 2000` working. Sessions are stateless by default so any worker or task can answer; `mcp.sessions: 'stateful'` keeps in-memory sessions (single worker only) that close after `sessionIdleTimeoutMs` (default 15 minutes) idle. A failed `authenticate` returns 401 with `WWW-Authenticate: Bearer`, and hyper-express now applies `authenticate`. Tools forward the caller's `Authorization`, `x-correlation-id` and trace headers to the routes they call; a header the tool supplies wins.
