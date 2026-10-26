{{#is_iam}}import type { IamSdkClient{{#is_better_auth}}, BetterAuthConfig{{/is_better_auth}} } from "@{{app_name}}/iam";{{/is_iam}}{{#is_billing}}
import type { BillingSdkClient } from "@{{app_name}}/billing";{{/is_billing}}{{#is_messaging}}
import type { MessagingSdkClient } from "@{{app_name}}/messaging";{{/is_messaging}}{{#is_cac}}
import type { CacSdkClient } from "@{{app_name}}/cac";{{/is_cac}}{{#is_ecommerce}}
import type { EcommerceSdkClient } from "@{{app_name}}/ecommerce";{{/is_ecommerce}}
import { {{#is_better_auth}}RegistryOptions, {{/is_better_auth}}universalSdk } from "@forklaunch/universal-sdk";{{#is_better_auth}}
import { createAuthClient } from "better-auth/client";{{/is_better_auth}}
{{#is_better_auth}}import { inferAdditionalFields } from 'better-auth/client/plugins';{{/is_better_auth}}
{{#is_iam}}
//! export various service and worker sdk clients
export const iamSdkClient{{^is_better_auth}}: typeof universalSdk<IamSdkClient>{{/is_better_auth}} = {{#is_better_auth}}async ({
    host,
    registryOptions,
    coreHost,
    authBasePath
}: {
    host: string,
    registryOptions: RegistryOptions,
    authBasePath?: string,
    coreHost?: string
}) => ({
    core: await {{/is_better_auth}}universalSdk<IamSdkClient>{{#is_better_auth}}({
        host: coreHost ?? host,
        registryOptions
    }),
    betterAuth: createAuthClient({
        baseURL: host,
        basePath: authBasePath,
        // The Better Auth client always sends `credentials: 'include'`, so every
        // request is cookie-bearing and hits Better Auth's origin/CSRF guard.
        // Browsers set `Origin` automatically; non-browser runtimes (Node SDK
        // consumers, e2e tests) do not, so a cookie-bearing request would arrive
        // with a null Origin and be rejected (MISSING_OR_NULL_ORIGIN). Present
        // this client's own API origin — which Better Auth trusts as its
        // `baseURL`. Browsers ignore attempts to set the forbidden `Origin`
        // header (using the real one), so this is correct in every runtime.
        fetchOptions: {
            headers: { origin: new URL(host).origin }
        },
        plugins: [inferAdditionalFields<BetterAuthConfig>()]
    })
}){{/is_better_auth}};{{/is_iam}}{{#is_billing}}
export const billingSdkClient: typeof universalSdk<BillingSdkClient> = universalSdk<BillingSdkClient>;{{/is_billing}}{{#is_messaging}}
export const messagingSdkClient: typeof universalSdk<MessagingSdkClient> = universalSdk<MessagingSdkClient>;{{/is_messaging}}{{#is_cac}}
export const cacSdkClient: typeof universalSdk<CacSdkClient> = universalSdk<CacSdkClient>;{{/is_cac}}{{#is_ecommerce}}
export const ecommerceSdkClient: typeof universalSdk<EcommerceSdkClient> = universalSdk<EcommerceSdkClient>;{{/is_ecommerce}}
