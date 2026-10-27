import { useState } from 'react';
import { ScrollView, Text, TextInput, Button } from 'react-native';
import { useAssistant } from './useAssistant';
export function Assistant() {
  const [draft, setDraft] = useState('');
  const { messages, result, busy, error, send } = useAssistant();
  return <ScrollView contentContainerStyle={{ padding: 24, gap: 16 }} keyboardShouldPersistTaps="handled">{messages.map((m, i) => <Text key={i}>{m.role}: {m.text}</Text>)}{result?.toolCalls.map(t => <Text key={t.id}>{t.name}: {t.ok ? 'Completed' : 'Could not complete'}</Text>)}{result?.approval && <><Text accessibilityRole="header">Review this action</Text><Text>{result.approval.name}</Text><Text>{JSON.stringify(result.approval.arguments, null, 2)}</Text><Button disabled={busy} title="Allow this action" onPress={() => void send({ continuation: result.approval!.continuation, decision: 'approve' })}/><Button disabled={busy} title="Cancel" onPress={() => void send({ continuation: result.approval!.continuation, decision: 'deny' })}/></>}<TextInput accessibilityLabel="Your message" placeholder="Ask your assistant…" multiline maxLength={8000} value={draft} onChangeText={setDraft}/><Button disabled={busy || !draft.trim() || !!result?.approval} title={busy ? 'Working…' : 'Send'} onPress={() => void send({ task: draft })}/>{error ? <Text accessibilityRole="alert">{error}</Text> : null}</ScrollView>;
}
