import { optional, string } from '@{{app_name}}/core';
import { defineTool } from './index';

// An example read-only tool. Replace or delete it.
export const currentTime = defineTool({
  name: 'current_time',
  title: 'Current time',
  description:
    'The current date and time, optionally in an IANA time zone such as America/New_York.',
  inputSchema: { timeZone: optional(string) },
  annotations: { readOnlyHint: true, idempotentHint: true },
  handler: async ({ timeZone }: { timeZone?: string }) => {
    try {
      return {
        now: new Date().toLocaleString('en-US', {
          timeZone: timeZone || 'UTC',
          dateStyle: 'full',
          timeStyle: 'long'
        })
      };
    } catch {
      throw new Error(`Unknown time zone: ${timeZone}`);
    }
  }
});
