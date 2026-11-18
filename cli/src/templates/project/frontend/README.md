# Web app

Run `pnpm install` and `pnpm dev`. `pnpm build` checks types and builds the website. Point the public `VITE_IAM_URL` at the shared Better Auth service, with its allowed origin and cookie settings configured for this website. It may be a full address or a path on this website; left empty, the website reaches IAM on its own address, as a published ForkLaunch website does. The ForkLaunch sandbox supplies the address during preview.

Sign-in uses server-managed cookies. Keep provider keys and backend secrets on the server. Add product screens here and call the same authorized backend operations from the UI and optional assistant.
