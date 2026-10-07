import { config } from 'dotenv';
import { existsSync } from 'fs';

export class UndeclaredSecretError extends Error {
  readonly name = 'UndeclaredSecretError' as const;

  constructor(key: string) {
    super(`Secret "${key}" is not declared in the manifest.`);
  }
}

export class MissingSecretError extends Error {
  readonly name = 'MissingSecretError' as const;

  constructor(key: string) {
    super(`Secret "${key}" is declared but not present in the environment.`);
  }
}

export class SecretsAccessor {
  private readonly declaredSecrets: Set<string>;
  private readonly localSecretsFile: string | undefined;

  constructor(declaredSecrets: string[], localSecretsFile?: string) {
    this.declaredSecrets = new Set(declaredSecrets);
    this.localSecretsFile = localSecretsFile;
  }

  validateAtBoot(): void {
    this.loadLocalSecretsIfApplicable();

    const missing: string[] = [];
    for (const key of this.declaredSecrets) {
      if (!process.env[key]) {
        missing.push(key);
      }
    }

    if (missing.length > 0) {
      process.stderr.write(`Missing required secrets: ${missing.join(', ')}\n`);
      process.exit(1);
    }
  }

  getSecret(key: string): string {
    if (!this.declaredSecrets.has(key)) {
      throw new UndeclaredSecretError(key);
    }

    const value = process.env[key];
    if (!value) {
      throw new MissingSecretError(key);
    }

    return value;
  }

  private loadLocalSecretsIfApplicable(): void {
    if (
      process.env['NODE_ENV'] !== 'production' &&
      this.localSecretsFile &&
      existsSync(this.localSecretsFile)
    ) {
      config({ path: this.localSecretsFile });
    }
  }
}
