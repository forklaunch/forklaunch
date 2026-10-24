/** MCP tool annotations (https://modelcontextprotocol.io/specification). */
// A type alias (not an interface) so it fits Record<string, unknown> response schemas.
export type ToolAnnotations = {
  title?: string;
  readOnlyHint?: boolean;
  destructiveHint?: boolean;
  idempotentHint?: boolean;
  openWorldHint?: boolean;
};

export type JsonSchema = Record<string, unknown>;

/** Per-call context. `authorization` is the caller's own header, forwarded to app services. */
export interface AgentToolContext {
  userId: string;
  authorization?: string;
  signal: AbortSignal;
}

export type ToolInputParse<I> = (
  input: unknown
) => { ok: true; value: I } | { ok: false; message: string };

/** A tool the model can call, in the MCP tools/list shape plus its handler. */
export interface AgentTool<I = unknown> {
  name: string;
  title?: string;
  description: string;
  inputSchema: JsonSchema;
  annotations: ToolAnnotations;
  parse?: ToolInputParse<I>;
  handler: (input: I, context: AgentToolContext) => Promise<unknown>;
}

export type ToolSourceKind = 'local' | 'service' | 'mcp';

/** A provider of tools: the agent's own code, an app service or an MCP server. */
export interface ToolSource {
  kind: ToolSourceKind;
  name: string;
  /** Tool definitions. Called with the caller's context; may be cached by the source. */
  /** `fresh` skips any cache; `GET /tools` asks for it, runs reuse the 5-minute cache. */
  list(
    context: AgentToolContext,
    options?: { fresh?: boolean }
  ): Promise<AgentTool[]>;
}

/** One entry of `GET /api/v1/agent/tools`. */
export interface ListedTool {
  name: string;
  title?: string;
  description: string;
  inputSchema: JsonSchema;
  annotations: ToolAnnotations;
  source: { kind: ToolSourceKind; name: string };
  requiresApproval: boolean;
  available: boolean;
  unavailableReason: string | null;
}

export interface AgentToolListing {
  agent: string;
  model: string | null;
  tools: ListedTool[];
}

export interface AgentLimits {
  /** Model calls that may request tools in one run. Default 8. */
  maxSteps: number;
  /** Characters accepted in a task. Default 8,000. */
  maxTaskChars: number;
  /** Output tokens per model call. Default 1,024. */
  maxOutputTokens: number;
  /** Milliseconds per tool call. Default 30,000. */
  toolTimeoutMs: number;
  /** Characters of one tool result passed to the model. Default 65,536. */
  maxToolResultChars: number;
}

/** Events of `POST /api/v1/agent/tasks/stream`; one JSON object per `data:` line. */
export type AgentEvent =
  | { type: 'status'; message: string }
  | { type: 'delta'; text: string }
  | { type: 'tool_call'; id: string; name: string; arguments: unknown }
  | { type: 'tool_result'; id: string; ok: boolean; summary: string }
  | {
      type: 'approval_required';
      id: string;
      name: string;
      arguments: unknown;
      annotations: ToolAnnotations;
      continuation: string;
    }
  | { type: 'done'; model: string }
  | { type: 'error'; message: string };

/** Body of the task routes: start a task, or resume one after an approval. */
export type AgentTaskRequest =
  { task: string } | { continuation: string; decision: 'approve' | 'deny' };

export interface AgentTaskResult {
  answer: string | null;
  model: string;
  toolCalls: { id: string; name: string; ok: boolean }[];
  approval?: Extract<AgentEvent, { type: 'approval_required' }>;
}

export class AgentError extends Error {
  constructor(
    readonly status: 400 | 401 | 429 | 502 | 503,
    message: string
  ) {
    super(message);
    this.name = 'AgentError';
  }
}
