import { describe, expect, it } from '{{#is_vitest}}vitest{{/is_vitest}}{{#is_jest}}@jest/globals{{/is_jest}}';
import { defineAgent, localTools, toolFactory } from '@forklaunch/core/agent';
import { SchemaValidator, string } from '@{{app_name}}/core';
import { currentTime } from '../tools/currentTime.tool';

const context = {
  userId: 'user-1',
  authorization: 'Bearer test',
  signal: new AbortController().signal
};

describe('{{service_name}} agent tools', () => {
  it('reports each tool in the MCP shape with honest approval flags', async () => {
    const changeNote = toolFactory(SchemaValidator())({
      name: 'change_note',
      description: 'Changes a note.',
      inputSchema: { text: string },
      annotations: { readOnlyHint: false },
      handler: async () => ({ changed: true })
    });
    const agent = defineAgent({
      name: '{{service_name}}',
      instructions: 'Test.',
      model: 'test-model',
      sources: [localTools('{{service_name}}', [currentTime, changeNote])],
      continuationSecret: 'test-secret'
    });
    const listing = await agent.listTools(context);
    // Listings are sorted by tool name.
    expect(listing.tools.map((t) => [t.name, t.requiresApproval])).toEqual([
      ['change_note', true],
      ['current_time', false]
    ]);
  });

  it('answers current_time and rejects unknown time zones', async () => {
    await expect(currentTime.handler({ timeZone: 'UTC' }, context)).resolves.toHaveProperty('now');
    await expect(currentTime.handler({ timeZone: 'Not/AZone' }, context)).rejects.toThrow(/Unknown time zone/);
  });
});
