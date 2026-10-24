import {
  array,
  boolean,
  handlers,
  null_,
  optional,
  PLATFORM_VIEWER_ROLES,
  record,
  schemaValidator,
  string,
  union,
  unknown
} from '@{{app_name}}/core';
import {
  AgentError,
  type AgentTaskRequest,
  type AgentToolContext
} from '@forklaunch/core/agent';
import { agent } from '../../agent.config';
import { ci, tokens } from '../../bootstrapper';

const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// Anyone signed in may use the agent. Each tool call still goes through the
// called service's own roles, because the caller's token is forwarded.
const auth = {
  jwt: { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL },
  allowedRoles: PLATFORM_VIEWER_ROLES
};

// Response shapes of the shared agent contract (docs/design/init-agent.md).
const json = record(string, unknown);
const toolListing = {
  agent: string,
  model: union([string, null_]),
  tools: array({
    name: string,
    title: optional(string),
    description: string,
    inputSchema: json,
    annotations: json,
    source: { kind: string, name: string },
    requiresApproval: boolean,
    available: boolean,
    unavailableReason: union([string, null_])
  })
};
const taskResult = {
  answer: union([string, null_]),
  model: string,
  toolCalls: array({ id: string, name: string, ok: boolean }),
  approval: optional({
    type: string,
    id: string,
    name: string,
    arguments: unknown,
    annotations: json,
    continuation: string
  })
};

// Either a new task, or a decision on a paused tool call.
const taskBody = {
  task: optional(string),
  continuation: optional(string),
  decision: optional(string)
};

function toRequest(body: {
  task?: string;
  continuation?: string;
  decision?: string;
}): AgentTaskRequest {
  if (body.continuation) {
    if (body.decision !== 'approve' && body.decision !== 'deny')
      throw new AgentError(400, 'Choose approve or deny.');
    return { continuation: body.continuation, decision: body.decision };
  }
  return { task: body.task ?? '' };
}

function toolContext(
  req: { session?: { sub?: string }; headers: Record<string, unknown> },
  signal: AbortSignal
): AgentToolContext {
  const userId = req.session?.sub;
  if (!userId) throw new AgentError(401, 'Sign in to use the agent.');
  const authorization = req.headers.authorization;
  return {
    userId,
    authorization: typeof authorization === 'string' ? authorization : undefined,
    signal
  };
}

export const agentTools = handlers.get(
  schemaValidator,
  '/tools',
  {
    name: 'ListAgentTools',
    summary: 'Lists the tools this agent can use, in the MCP tools/list shape',
    access: 'protected',
    auth,
    responses: { 200: toolListing, 401: string }
  },
  async (req, res) => {
    try {
      const abort = new AbortController();
      res.status(200).json(await agent.listTools(toolContext(req, abort.signal)));
    } catch (error) {
      if (error instanceof AgentError) {
        res.status(401).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const agentTask = handlers.post(
  schemaValidator,
  '/tasks',
  {
    name: 'RunAgentTask',
    summary: 'Runs a task, or resumes one after an approval decision',
    access: 'protected',
    auth,
    body: taskBody,
    responses: {
      200: taskResult,
      400: string,
      401: string,
      429: string,
      502: string,
      503: string
    }
  },
  async (req, res) => {
    const abort = new AbortController();
    res.on('close', () => abort.abort());
    try {
      res
        .status(200)
        .json(await agent.run(toRequest(req.body), toolContext(req, abort.signal)));
    } catch (error) {
      if (error instanceof AgentError) {
        res.status(error.status).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const agentTaskStream = handlers.post(
  schemaValidator,
  '/tasks/stream',
  {
    name: 'StreamAgentTask',
    summary: 'Streams a task, or resumes one after an approval decision',
    access: 'protected',
    auth,
    body: taskBody,
    responseHeaders: {
      'Content-Type': string,
      'Cache-Control': string,
      'X-Accel-Buffering': string
    },
    responses: { 200: string, 400: string, 401: string }
  },
  async (req, res) => {
    const abort = new AbortController();
    res.on('close', () => abort.abort());
    let request: AgentTaskRequest;
    let context: AgentToolContext;
    try {
      request = toRequest(req.body);
      context = toolContext(req, abort.signal);
    } catch (error) {
      if (error instanceof AgentError) {
        res.status(error.status === 401 ? 401 : 400).send(error.message);
        return;
      }
      throw error;
    }
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-store');
    res.setHeader('X-Accel-Buffering', 'no');
    res.flushHeaders();
    for await (const event of agent.stream(request, context)) {
      if (res.destroyed) break;
      res.write(`data: ${JSON.stringify(event)}\n\n`);
    }
    res.end();
  }
);
