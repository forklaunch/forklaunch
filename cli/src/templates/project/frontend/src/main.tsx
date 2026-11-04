import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { HashRouter, Routes, Route, NavLink } from 'react-router-dom';
import { auth } from './auth';
import { productName } from './product';
import './styles.css';

function Account() {
  const { data: session, isPending, error: sessionError, refetch } = auth.useSession();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  if (isPending) return <p role="status">Checking your account…</p>;
  if (sessionError) return <section><p role="alert">We couldn’t connect to your account.</p><button onClick={() => void refetch()}>Try again</button></section>;
  if (session) return <section><h1>Your account</h1><p>Signed in as {session.user.email}</p><button disabled={busy} onClick={async () => { setBusy(true); try { const result = await auth.signOut(); if (result.error) setError('You could not be signed out. Try again.'); } catch { setError('Check your connection and try again.'); } finally { setBusy(false); } }}>Sign out</button>{error && <p role="alert">{error}</p>}</section>;
  return <form onSubmit={async event => { event.preventDefault(); setBusy(true); setError(''); try { const result = await auth.signIn.email({ email, password }); if (result.error) setError('Check your email and password and try again.'); else setPassword(''); } catch { setError('Check your connection and try again.'); } finally { setBusy(false); } }}>
    <h1>Sign in</h1><label>Email<input type="email" autoComplete="email" required value={email} onChange={e => setEmail(e.target.value)}/></label><label>Password<input type="password" autoComplete="current-password" required value={password} onChange={e => setPassword(e.target.value)}/></label><button disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button>{error && <p role="alert">{error}</p>}
  </form>;
}
function App() {
  return <HashRouter><a className="skip" href="#main">Skip to content</a><header><strong>{productName}</strong><nav aria-label="Main navigation"><NavLink to="/">Home</NavLink><NavLink to="/account">Account</NavLink></nav></header><main id="main"><Routes><Route path="/" element={<section><h1>{productName}</h1><p>Your app is ready to take shape.</p></section>}/><Route path="/account" element={<Account/>}/><Route path="*" element={<section><h1>Page not found</h1><NavLink to="/">Go home</NavLink></section>}/></Routes></main></HashRouter>;
}
document.title = productName;
createRoot(document.getElementById('root')!).render(<React.StrictMode><App/></React.StrictMode>);
