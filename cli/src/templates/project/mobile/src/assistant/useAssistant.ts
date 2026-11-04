import { useState } from 'react';
import { runTask, type Task, type TaskResult } from './api';
export function useAssistant() {
  const [messages, setMessages] = useState<{ role: string; text: string }[]>([]);
  const [result, setResult] = useState<TaskResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const send = async (body: Task) => {
    if (busy) return;
    setBusy(true); setError('');
    try {
      const next = await runTask(body);
      setResult(next);
      setMessages(previous => [...previous, ...('task' in body ? [{ role: 'You', text: body.task }] : []), ...(next.answer ? [{ role: 'Assistant', text: next.answer }] : [])]);
    } catch (reason) { setError(reason instanceof Error ? reason.message : 'Your message could not be sent. Try again.'); }
    finally { setBusy(false); }
  };
  return { messages, result, busy, error, send };
}
