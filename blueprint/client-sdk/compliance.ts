import { clientBillingSdkClient, clientIamSdkClient } from './clientSdk';

type IamClient = Awaited<ReturnType<typeof clientIamSdkClient>>;
type BillingClient = Awaited<ReturnType<typeof clientBillingSdkClient>>;

/**
 * Compliance fan-out client. Calls erase/export on every registered service
 * in parallel. Per-service responses are the exact discriminated unions
 * produced by each SDK (e.g. `{ code: 200; response: {...} } | { code: 404; response: string }`),
 * so callers narrow on `code` without any casts.
 *
 * Requires a JWT token from a user with SYSTEM role.
 */
export function createComplianceClient(config: {
  token: string;
  iam: IamClient;
  billing: BillingClient;
}) {
  const headers = { authorization: `Bearer ${config.token}` } as const;

  return {
    async erase(userId: string) {
      const [iam, billing] = await Promise.all([
        config.iam.compliance.eraseUserData({ params: { userId }, headers }),
        config.billing.compliance.eraseUserData({ params: { userId }, headers })
      ]);
      return { iam, billing };
    },

    async export(userId: string) {
      const [iam, billing] = await Promise.all([
        config.iam.compliance.exportUserData({ params: { userId }, headers }),
        config.billing.compliance.exportUserData({
          params: { userId },
          headers
        })
      ]);
      return { iam, billing };
    }
  };
}
