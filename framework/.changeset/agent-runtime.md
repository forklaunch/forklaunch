---
'@forklaunch/core': minor
---

Add `@forklaunch/core/agent`: the runtime behind `forklaunch init agent`. Tools in the MCP shape (`toolFactory`, `localTools`), app services as tools from their OpenAPI documents (`serviceTools`), external MCP servers over streamable HTTP (`mcpServer`), a tool-calling loop over the model gateway with streaming events, and stateless approvals for tools that are not read-only (AES-GCM sealed continuations).
