import { optional, string, SchemaValidator } from '@forklaunch/validator/zod';
import { describe, expect, it } from 'vitest';
import {
  defineAgent,
  localTools,
  mcpServer,
  openContinuation,
  serviceTools,
  toolFactory,
  type AgentEvent
} from '../src/agent';
import type {
  GatewayChatChunk,
  GatewayChatCompletion,
  GatewayChatRequest,
  ModelGatewayClient
} from '../src/http/modelGatewayClient';

const defineTool = toolFactory(SchemaValidator());
const context = (userId = 'user-1') => ({
  userId,
  authorization: 'Bearer user-token',
  signal: new AbortController().signal
});
const SECRET = 'test-hmac-secret';

type Turn =
  | { content: string }
  | { calls: { name: string; arguments: Record<string, unknown> }[] };

/** Plays scripted model turns and records each request. */
function scripted(turns: Turn[]) {
  const requests: GatewayChatRequest[] = [];
  let index = 0;
  const next = (request: GatewayChatRequest) => {
    requests.push(structuredClone(request));
    const turn = turns[index++];
    if (!turn) throw new Error('No scripted turn left');
    return turn;
  };
  const client: ModelGatewayClient = {
    models: async () => ({
      models: [],
      monthlyTokenBudget: 0,
      tokensUsedThisMonth: 0,
      requestsPerMinute: 0
    }),
    chat: {
      completions: {
        async create(request) {
          const turn = next(request);
          return {
            id: 'c',
            object: 'chat.completion',
            created: 0,
            model: request.model,
            choices: [
              {
                index: 0,
                finish_reason: 'calls' in turn ? 'tool_calls' : 'stop',
                message:
                  'calls' in turn
                    ? {
                        role: 'assistant',
                        content: null,
                        tool_calls: turn.calls.map((call, i) => ({
                          id: `call_${index}_${i}`,
                          type: 'function',
                          function: {
                            name: call.name,
                            arguments: JSON.stringify(call.arguments)
                          }
                        }))
                      }
                    : { role: 'assistant', content: turn.content }
              }
            ]
          } satisfies GatewayChatCompletion;
        },
        async *stream(request) {
          const turn = next(request);
          const chunk = (
            delta: GatewayChatChunk['choices'][0]['delta'],
            finish?: string
          ) =>
            ({
              id: 'c',
              object: 'chat.completion.chunk',
              model: request.model,
              choices: [{ index: 0, delta, finish_reason: finish ?? null }]
            }) satisfies GatewayChatChunk;
          if ('content' in turn) {
            for (const part of turn.content.match(/.{1,4}/g) ?? [])
              yield chunk({ content: part });
            yield chunk({}, 'stop');
            return;
          }
          for (const [i, call] of turn.calls.entries()) {
            const args = JSON.stringify(call.arguments);
            yield chunk({
              tool_calls: [
                {
                  index: i,
                  id: `call_${i}`,
                  function: { name: call.name, arguments: args.slice(0, 3) }
                }
              ]
            });
            yield chunk({
              tool_calls: [{ index: i, function: { arguments: args.slice(3) } }]
            });
          }
          yield chunk({}, 'tool_calls');
        }
      }
    }
  };
  return { client: () => client, requests };
}

const ran: string[] = [];
const lookup = defineTool({
  name: 'lookup_order',
  description: 'Look up an order by id.',
  inputSchema: { orderId: string, note: optional(string) },
  annotations: { readOnlyHint: true },
  handler: async (input: { orderId: string }) => {
    ran.push(`lookup:${input.orderId}`);
    return { orderId: input.orderId, status: 'shipped' };
  }
});
const cancel = defineTool({
  name: 'cancel_order',
  description: 'Cancel an order.',
  inputSchema: { orderId: string },
  annotations: { destructiveHint: true },
  handler: async (input: { orderId: string }) => {
    ran.push(`cancel:${input.orderId}`);
    return { cancelled: true };
  }
});

const agent = (turns: Turn[], streaming = false) => {
  const model = scripted(turns);
  return {
    model,
    agent: defineAgent({
      name: 'assistant',
      instructions: 'Help with orders.',
      model: 'luna',
      sources: [localTools('assistant', [lookup, cancel])],
      continuationSecret: SECRET,
      streaming,
      client: model.client
    })
  };
};

async function events(stream: AsyncGenerator<AgentEvent>) {
  const out: AgentEvent[] = [];
  for await (const event of stream) out.push(event);
  return out;
}

describe('agent', () => {
  it('lists tools in the MCP shape with approval flags', async () => {
    const listing = await agent([]).agent.listTools(context());
    expect(listing.agent).toBe('assistant');
    expect(listing.model).toBe('luna');
    // Sorted by name, like the Rust runtime.
    expect(listing.tools.map((t) => [t.name, t.requiresApproval])).toEqual([
      ['cancel_order', true],
      ['lookup_order', false]
    ]);
    expect(listing.tools[1]!.inputSchema).toMatchObject({
      type: 'object',
      properties: { orderId: { type: 'string' } }
    });
    expect(listing.tools[1]!.source).toEqual({
      kind: 'local',
      name: 'assistant'
    });
  });

  it('runs read-only tools without asking and offers them to the model', async () => {
    ran.length = 0;
    const { agent: a, model } = agent([
      { calls: [{ name: 'lookup_order', arguments: { orderId: 'A1' } }] },
      { content: 'Order A1 has shipped.' }
    ]);
    const result = await a.run({ task: 'Where is A1?' }, context());
    expect(result.answer).toBe('Order A1 has shipped.');
    expect(ran).toEqual(['lookup:A1']);
    expect(model.requests[0]!.tools).toHaveLength(2);
    const toolMessage = model.requests[1]!.messages.find(
      (m) => m.role === 'tool'
    );
    expect(JSON.parse(String(toolMessage!.content))).toEqual({
      orderId: 'A1',
      status: 'shipped'
    });
  });

  it('pauses before a tool that changes data and resumes after approval', async () => {
    ran.length = 0;
    const { agent: a } = agent([
      { calls: [{ name: 'cancel_order', arguments: { orderId: 'B2' } }] },
      { content: 'Cancelled B2.' }
    ]);
    const first = await a.run({ task: 'Cancel B2' }, context());
    expect(first.answer).toBeNull();
    expect(first.approval).toMatchObject({
      name: 'cancel_order',
      arguments: { orderId: 'B2' }
    });
    expect(ran).toEqual([]);
    const second = await a.run(
      { continuation: first.approval!.continuation, decision: 'approve' },
      context()
    );
    expect(ran).toEqual(['cancel:B2']);
    expect(second.answer).toBe('Cancelled B2.');
    expect(second.toolCalls).toEqual([
      { id: first.approval!.id, name: 'cancel_order', ok: true }
    ]);
  });

  it('tells the model when the user declines, without running the tool', async () => {
    ran.length = 0;
    const { agent: a, model } = agent([
      { calls: [{ name: 'cancel_order', arguments: { orderId: 'C3' } }] },
      { content: 'I did not cancel C3.' }
    ]);
    const first = await a.run({ task: 'Cancel C3' }, context());
    const second = await a.run(
      { continuation: first.approval!.continuation, decision: 'deny' },
      context()
    );
    expect(ran).toEqual([]);
    expect(second.answer).toBe('I did not cancel C3.');
    expect(JSON.stringify(model.requests[1]!.messages)).toContain('declined');
  });

  it('rejects continuations from another user or that were changed', async () => {
    const { agent: a } = agent([
      { calls: [{ name: 'cancel_order', arguments: { orderId: 'D4' } }] }
    ]);
    const first = await a.run({ task: 'Cancel D4' }, context());
    const token = first.approval!.continuation;
    expect(openContinuation(token, SECRET, 'user-2')).toBeNull();
    expect(
      openContinuation(`${token.slice(0, -2)}xx`, SECRET, 'user-1')
    ).toBeNull();
    expect(openContinuation(token, 'other-secret', 'user-1')).toBeNull();
    await expect(
      a.run({ continuation: token, decision: 'approve' }, context('user-2'))
    ).rejects.toThrow(/expired or belongs to another session/);
  });

  it('streams deltas, joins streamed tool-call fragments and reports each step', async () => {
    ran.length = 0;
    const { agent: a } = agent(
      [
        { calls: [{ name: 'lookup_order', arguments: { orderId: 'E5' } }] },
        { content: 'E5 has shipped.' }
      ],
      true
    );
    const out = await events(a.stream({ task: 'E5?' }, context()));
    expect(out.map((e) => e.type)).toEqual([
      'status',
      'tool_call',
      'tool_result',
      ...Array(4).fill('delta'),
      'done'
    ]);
    expect(out[1]).toMatchObject({
      name: 'lookup_order',
      arguments: { orderId: 'E5' }
    });
    expect(ran).toEqual(['lookup:E5']);
  });

  it('reports invalid tool input to the model instead of running the tool', async () => {
    ran.length = 0;
    const { agent: a, model } = agent([
      { calls: [{ name: 'lookup_order', arguments: { orderId: 7 } }] },
      { content: 'I need a valid order id.' }
    ]);
    const out = await events(a.stream({ task: 'x' }, context()));
    expect(out.find((e) => e.type === 'tool_result')).toMatchObject({
      ok: false
    });
    expect(ran).toEqual([]);
    expect(JSON.stringify(model.requests[1]!.messages)).toContain('orderId');
  });

  it('forces a final answer without tools after the step limit', async () => {
    const turns: Turn[] = Array(8).fill({
      calls: [{ name: 'lookup_order', arguments: { orderId: 'F' } }]
    });
    const { agent: a, model } = agent([...turns, { content: 'Done.' }]);
    const result = await a.run({ task: 'loop' }, context());
    expect(result.answer).toBe('Done.');
    expect(model.requests.at(-1)!.tools).toBeUndefined();
  });

  it('refuses empty and oversized tasks', async () => {
    const { agent: a } = agent([]);
    await expect(a.run({ task: '  ' }, context())).rejects.toMatchObject({
      status: 400
    });
    await expect(
      a.run({ task: 'x'.repeat(8001) }, context())
    ).rejects.toMatchObject({ status: 400 });
  });

  it('turns an app service OpenAPI document into tools called with the caller’s auth', async () => {
    const calls: {
      url: string;
      method: string;
      auth: string | null;
      body?: string;
    }[] = [];
    const fetchStub = (async (input: string | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith('/api/v1/openapi'))
        return Response.json({
          latest: {
            paths: {
              '/records/{id}': {
                get: {
                  operationId: 'GetRecord',
                  summary: 'Get a record',
                  parameters: [
                    { name: 'id', in: 'path', schema: { type: 'string' } }
                  ]
                },
                delete: {
                  operationId: 'DeleteRecord',
                  parameters: [{ name: 'id', in: 'path' }]
                }
              },
              '/records': {
                post: {
                  operationId: 'CreateRecord',
                  requestBody: {
                    content: {
                      'application/json': {
                        schema: { $ref: '#/components/schemas/New' }
                      }
                    }
                  }
                }
              },
              '/api/v1/openapi': { get: { operationId: 'Spec' } },
              '/health': { get: { operationId: 'Health' } }
            },
            components: {
              schemas: {
                New: {
                  type: 'object',
                  properties: { title: { type: 'string' } }
                }
              }
            }
          }
        });
      const headers = new Headers(init?.headers);
      calls.push({
        url,
        method: init?.method ?? 'GET',
        auth: headers.get('authorization'),
        body: init?.body as string
      });
      return Response.json({ id: 'r1' });
    }) as typeof fetch;
    const source = serviceTools('records', {
      url: 'http://records:8000/',
      fetch: fetchStub
    });
    const tools = await source.list(context());
    expect(tools.map((t) => [t.name, t.annotations])).toEqual([
      ['records__get_record', { readOnlyHint: true }],
      [
        'records__delete_record',
        { readOnlyHint: false, destructiveHint: true }
      ],
      ['records__create_record', { readOnlyHint: false }]
    ]);
    expect(tools[2]!.inputSchema).toMatchObject({
      properties: {
        body: { type: 'object', properties: { title: { type: 'string' } } }
      },
      required: ['body']
    });
    await tools[0]!.handler({ params: { id: 'a b' } }, context());
    await tools[2]!.handler({ body: { title: 'x' } }, context());
    expect(calls).toEqual([
      {
        url: 'http://records:8000/records/a%20b',
        method: 'GET',
        auth: 'Bearer user-token',
        body: undefined
      },
      {
        url: 'http://records:8000/records',
        method: 'POST',
        auth: 'Bearer user-token',
        body: '{"title":"x"}'
      }
    ]);
  });

  it('lists an unreachable source as unavailable instead of failing', async () => {
    const a = defineAgent({
      name: 'assistant',
      instructions: 'x',
      model: 'luna',
      sources: [
        localTools('assistant', [lookup]),
        serviceTools('billing', {
          url: 'http://billing:8000',
          fetch: (async () => new Response('', { status: 500 })) as typeof fetch
        })
      ],
      continuationSecret: SECRET
    });
    const listing = await a.listTools(context());
    expect(listing.tools.map((t) => [t.name, t.available])).toEqual([
      ['billing__unavailable', false],
      ['lookup_order', true]
    ]);
    expect(listing.tools[0]!.unavailableReason).toMatch(/billing/);
  });
});

describe('external MCP servers', () => {
  it('lists and calls tools on a real streamable-HTTP MCP server', async () => {
    const { FastMCP } = await import('fastmcp');
    const { z } = await import('zod');
    const server = new FastMCP({ name: 'notes', version: '1.0.0' });
    const seen: string[] = [];
    server.addTool({
      name: 'searchNotes',
      description: 'Search notes by text.',
      parameters: z.object({ text: z.string() }),
      annotations: { readOnlyHint: true },
      execute: async ({ text }, context) => {
        seen.push(`${text}:${String(context.session === undefined)}`);
        return JSON.stringify({ hits: [`note about ${text}`] });
      }
    });
    const port = 20000 + Math.floor(Math.random() * 20000);
    await server.start({
      transportType: 'httpStream',
      // Explicit IPv4 on both ends: `localhost` can bind ::1 and dial 127.0.0.1.
      httpStream: { host: '127.0.0.1', port, endpoint: '/mcp' }
    });
    try {
      const source = mcpServer({
        name: 'notes',
        url: `http://127.0.0.1:${port}/mcp`,
        headers: { authorization: 'Bearer token', 'x-empty': undefined }
      });
      const tools = await source.list(context());
      expect(
        tools.map((t) => [
          t.name,
          t.annotations.readOnlyHint,
          t.annotations.openWorldHint
        ])
      ).toEqual([['notes__search_notes', true, true]]);
      expect(tools[0]!.inputSchema).toMatchObject({
        properties: { text: { type: 'string' } }
      });
      const result = await tools[0]!.handler({ text: 'tax' }, context());
      expect(JSON.stringify(result)).toContain('note about tax');
      expect(seen).toHaveLength(1);
    } finally {
      await server.stop();
    }
  }, 20000);
});
