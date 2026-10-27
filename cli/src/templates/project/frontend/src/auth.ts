import { createAuthClient } from 'better-auth/react';
import { jwtClient } from 'better-auth/client/plugins';
// Only a public service address belongs here. Sessions use server cookies.
export const auth = createAuthClient({ baseURL: import.meta.env.VITE_IAM_URL || window.location.origin, plugins: [jwtClient()] });
