import { createAuthClient } from 'better-auth/react';
import { jwtClient } from 'better-auth/client/plugins';
// Only a public service address belongs here. Sessions use server cookies.
// VITE_IAM_URL may be a full address, a path on this website (a preview is
// served under one) or empty (a published website reaches IAM on its own
// address). The sign-in client accepts only a full address, so resolve it
// against the page.
export const iamAddress = (setting: string | undefined, page: string) =>
  new URL(`${(setting || '').replace(/\/+$/, '')}/api/auth`, page).href;
export const auth = createAuthClient({ baseURL: iamAddress(import.meta.env.VITE_IAM_URL, window.location.origin), plugins: [jwtClient()] });
