import { useState } from 'react';
import { useAssistant } from './useAssistant';
export function Assistant() {
  const [draft, setDraft] = useState('');
  const { messages, result, busy, error, send } = useAssistant();
  return <section aria-label="Your assistant"><h1>Your assistant</h1><div role="log">{messages.map((m, i) => <p key={i}><strong>{m.role}: </strong>{m.text}</p>)}</div>{result?.toolCalls.map(t => <p key={t.id}>{t.name}: {t.ok ? 'Completed' : 'Could not complete'}</p>)}{result?.approval && <div><h2>Review this action</h2><p>{result.approval.name}</p><pre>{JSON.stringify(result.approval.arguments, null, 2)}</pre><button disabled={busy} onClick={() => void send({ continuation: result.approval!.continuation, decision: 'approve' })}>Allow this action</button><button disabled={busy} onClick={() => void send({ continuation: result.approval!.continuation, decision: 'deny' })}>Cancel</button></div>}<form onSubmit={async e => { e.preventDefault(); await send({ task: draft }); }}><label>Your message<textarea required maxLength={8000} value={draft} onChange={e => setDraft(e.target.value)}/></label><button disabled={busy || !draft.trim() || !!result?.approval}>{busy ? 'Working…' : 'Send'}</button></form>{error && <p role="alert">{error}</p>}</section>;
}
