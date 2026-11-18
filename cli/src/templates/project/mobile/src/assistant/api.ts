import { auth } from '../auth';
export type Approval = { name: string; arguments: unknown; continuation: string };
export type TaskResult = { answer: string | null; toolCalls: { id: string; name: string; ok: boolean }[]; approval?: Approval };
export type Task = { task: string } | { continuation: string; decision: 'approve' | 'deny' };
export async function runTask(body: Task): Promise<TaskResult> {
  const configured = process.env.EXPO_PUBLIC_AGENT_URL;
  if (!configured || !auth) throw new Error('Your assistant is still being connected. Please try again later.');
  const url = new URL(configured);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))) throw new Error('Your assistant needs a secure connection.');
  if (url.username || url.password || url.search || url.hash) throw new Error('Your assistant address needs to be checked.');
  // Tokens stay in memory, and go only to the configured service. The server
  // validates every tool and signs approvals; a button cannot grant permission.
  const token = await auth.token();
  if (!token.data?.token) throw new Error('Sign in before asking your assistant.');
  const response = await fetch(url.href.replace(/\/$/, '') + '/api/v1/agent/tasks', { method: 'POST', redirect: 'error', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token.data.token}` }, body: JSON.stringify(body) });
  if (!response.ok) throw new Error(response.status === 401 || response.status === 403 ? 'Sign in with an account that can use this assistant.' : 'Your assistant could not finish. Try again.');
  return await response.json() as TaskResult;
}
