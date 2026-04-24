{{#is_iam}}import type { IamSdkClient{{#is_better_auth}}, BetterAuthConfig{{/is_better_auth}} } from "@{{app_name}}/iam";{{/is_iam}}{{#is_billing}}
import type { BillingSdkClient } from "@{{app_name}}/billing";{{/is_billing}}
import { {{#is_better_auth}}RegistryOptions, {{/is_better_auth}}universalSdk } from "@forklaunch/universal-sdk";{{#is_better_auth}}
import { createAuthClient } from "better-auth/client";{{/is_better_auth}}
{{#is_better_auth}}import { inferAdditionalFields } from 'better-auth/client/plugins';{{/is_better_auth}}
{{#is_iam}}
//! export various service and worker sdk clients
export const iamSdkClient{{^is_better_auth}}{{/is_better_auth}} = {{#is_better_auth}}async ({
    host,
    registryOptions
}: {
    host: string,
    registryOptions: RegistryOptions
}) => ({
    core: await {{/is_better_auth}}universalSdk<IamSdkClient>{{#is_better_auth}}({
        host,
        registryOptions
    }),
    betterAuth: createAuthClient({
        baseURL: host,
        plugins: [inferAdditionalFields<BetterAuthConfig>()]
    })
}){{/is_better_auth}};{{/is_iam}}{{#is_billing}}
export const billingSdkClient = universalSdk<BillingSdkClient>;{{/is_billing}}
