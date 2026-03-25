import { generateHmacAuthHeaders } from '@forklaunch/core/http';

export interface ServiceComplianceResult<T> {
  service: string;
  status: 'fulfilled' | 'rejected';
  result?: T;
  error?: string;
}

interface EraseResult {
  entitiesAffected: string[];
  recordsDeleted: number;
}

interface ExportResult {
  userId: string;
  entities: Record<string, unknown[]>;
}

export interface ComplianceCapableService {
  compliance: {
    eraseUserData: (params: {
      params: { userId: string };
      headers: Record<string, string>;
    }) => Promise<{ code: number; response: unknown }>;
    exportUserData: (params: {
      params: { userId: string };
      headers: Record<string, string>;
    }) => Promise<{ code: number; response: unknown }>;
  };
}

/**
 * Fan-out compliance client. Calls erase/export on all registered services
 * in parallel and returns per-service results.
 *
 * Lives in client-sdk because it knows what services exist in the application.
 */
export function createComplianceClient(config: {
  hmacSecretKey: string;
  services: Record<string, ComplianceCapableService>;
}) {
  const { hmacSecretKey, services } = config;

  return {
    async erase(
      userId: string
    ): Promise<Record<string, ServiceComplianceResult<EraseResult>>> {
      const entries = Object.entries(services);
      const path = `/erase/${userId}`;
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'DELETE',
        path
      });

      const results = await Promise.allSettled(
        entries.map(async ([name, sdk]) => {
          const response = await sdk.compliance.eraseUserData({
            params: { userId },
            headers
          });
          return { name, response };
        })
      );

      const output: Record<string, ServiceComplianceResult<EraseResult>> = {};
      for (let i = 0; i < entries.length; i++) {
        const [name] = entries[i];
        const result = results[i];
        if (result.status === 'fulfilled') {
          output[name] = {
            service: name,
            status: 'fulfilled',
            result:
              result.value.response.code === 200
                ? (result.value.response.response as EraseResult)
                : { entitiesAffected: [], recordsDeleted: 0 }
          };
        } else {
          output[name] = {
            service: name,
            status: 'rejected',
            error: String(result.reason)
          };
        }
      }
      return output;
    },

    async export(
      userId: string
    ): Promise<Record<string, ServiceComplianceResult<ExportResult>>> {
      const entries = Object.entries(services);
      const path = `/export/${userId}`;
      const headers = generateHmacAuthHeaders({
        secretKey: hmacSecretKey,
        method: 'GET',
        path
      });

      const results = await Promise.allSettled(
        entries.map(async ([name, sdk]) => {
          const response = await sdk.compliance.exportUserData({
            params: { userId },
            headers
          });
          return { name, response };
        })
      );

      const output: Record<string, ServiceComplianceResult<ExportResult>> = {};
      for (let i = 0; i < entries.length; i++) {
        const [name] = entries[i];
        const result = results[i];
        if (result.status === 'fulfilled') {
          output[name] = {
            service: name,
            status: 'fulfilled',
            result:
              result.value.response.code === 200
                ? (result.value.response.response as ExportResult)
                : { userId, entities: {} }
          };
        } else {
          output[name] = {
            service: name,
            status: 'rejected',
            error: String(result.reason)
          };
        }
      }
      return output;
    }
  };
}
