import { useState } from 'react';
import { Button, Text, TextInput, ScrollView, StyleSheet } from 'react-native';
import { auth } from '../src/auth';
export default function Account() {
  if (!auth) return <Text style={styles.page}>Sign-in is being set up. Please try again later.</Text>;
  return <ConnectedAccount/>;
}
function ConnectedAccount() {
  const client = auth!;
  const { data: session, isPending, error: sessionError, refetch } = client.useSession();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const submit = async () => { setBusy(true); setError(''); try { const result = session ? await client.signOut() : await client.signIn.email({ email, password }); if (result.error) setError('Your request could not be completed. Check your details and try again.'); else setPassword(''); } catch { setError('Check your connection and try again.'); } finally { setBusy(false); } };
  if (isPending) return <Text style={styles.page}>Checking your account…</Text>;
  if (sessionError) return <ScrollView contentContainerStyle={styles.page}><Text>We couldn’t connect to your account.</Text><Button title="Try again" onPress={() => void refetch()}/></ScrollView>;
  return <ScrollView contentContainerStyle={styles.page} keyboardShouldPersistTaps="handled">{session ? <Text>Signed in as {session.user.email}</Text> : <><Text>Email</Text><TextInput accessibilityLabel="Email" style={styles.input} keyboardType="email-address" autoCapitalize="none" autoComplete="email" value={email} onChangeText={setEmail}/><Text>Password</Text><TextInput accessibilityLabel="Password" style={styles.input} secureTextEntry autoComplete="current-password" value={password} onChangeText={setPassword}/></>}<Button disabled={busy || (!session && (!email || !password))} title={busy ? 'Please wait…' : session ? 'Sign out' : 'Sign in'} onPress={() => void submit()}/>{error ? <Text accessibilityRole="alert">{error}</Text> : null}</ScrollView>;
}
const styles = StyleSheet.create({ page: { padding: 24, gap: 16 }, input: { borderWidth: 1, borderColor: '#aaa', borderRadius: 8, padding: 12, fontSize: 16 } });
