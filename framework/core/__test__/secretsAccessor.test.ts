import { existsSync } from 'fs';
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
  vi,
  type MockInstance
} from 'vitest';
import {
  MissingSecretError,
  SecretsAccessor,
  UndeclaredSecretError
} from '../src/secrets/secretsAccessor';

vi.mock('fs', async () => {
  const actual = await vi.importActual<typeof import('fs')>('fs');
  return {
    ...actual,
    existsSync: vi.fn()
  };
});

vi.mock('dotenv', () => ({
  config: vi.fn()
}));

describe('SecretsAccessor', () => {
  let originalEnv: NodeJS.ProcessEnv;
  let exitSpy: MockInstance;

  beforeEach(() => {
    originalEnv = { ...process.env };
    exitSpy = vi
      .spyOn(process, 'exit')
      .mockImplementation(() => undefined as never);
    vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
  });

  afterEach(() => {
    process.env = originalEnv;
    vi.restoreAllMocks();
  });

  describe('getSecret', () => {
    test('returns env var value for declared key', () => {
      process.env['MY_SECRET'] = 'secret-value';
      const accessor = new SecretsAccessor(['MY_SECRET']);

      expect(accessor.getSecret('MY_SECRET')).toBe('secret-value');
    });

    test('throws UndeclaredSecretError for undeclared key', () => {
      const accessor = new SecretsAccessor(['DECLARED_KEY']);

      expect(() => accessor.getSecret('UNDECLARED_KEY')).toThrow(
        UndeclaredSecretError
      );
    });

    test('throws MissingSecretError for declared key not in env', () => {
      delete process.env['MISSING_KEY'];
      const accessor = new SecretsAccessor(['MISSING_KEY']);

      expect(() => accessor.getSecret('MISSING_KEY')).toThrow(
        MissingSecretError
      );
    });
  });

  describe('validateAtBoot', () => {
    test('succeeds when all secrets present', () => {
      process.env['SECRET_A'] = 'a';
      process.env['SECRET_B'] = 'b';
      process.env['NODE_ENV'] = 'production';
      const accessor = new SecretsAccessor(['SECRET_A', 'SECRET_B']);

      accessor.validateAtBoot();

      expect(exitSpy).not.toHaveBeenCalled();
    });

    test('calls process.exit when secrets missing', () => {
      process.env['NODE_ENV'] = 'production';
      delete process.env['MISSING_A'];
      delete process.env['MISSING_B'];
      const accessor = new SecretsAccessor(['MISSING_A', 'MISSING_B']);

      accessor.validateAtBoot();

      expect(exitSpy).toHaveBeenCalledWith(1);
    });

    test('loads .secrets.local in non-production when file exists', async () => {
      const dotenv = await import('dotenv');
      process.env['NODE_ENV'] = 'development';
      vi.mocked(existsSync).mockReturnValue(true);

      const accessor = new SecretsAccessor([], '/app/.secrets.local');
      accessor.validateAtBoot();

      expect(existsSync).toHaveBeenCalledWith('/app/.secrets.local');
      expect(dotenv.config).toHaveBeenCalledWith({
        path: '/app/.secrets.local'
      });
    });

    test('does not load .secrets.local in production', async () => {
      const dotenv = await import('dotenv');
      vi.mocked(dotenv.config).mockClear();
      process.env['NODE_ENV'] = 'production';

      const accessor = new SecretsAccessor([], '/app/.secrets.local');
      accessor.validateAtBoot();

      expect(dotenv.config).not.toHaveBeenCalled();
    });

    test('does not load .secrets.local when file does not exist', async () => {
      const dotenv = await import('dotenv');
      vi.mocked(dotenv.config).mockClear();
      process.env['NODE_ENV'] = 'development';
      vi.mocked(existsSync).mockReturnValue(false);

      const accessor = new SecretsAccessor([], '/app/.secrets.local');
      accessor.validateAtBoot();

      expect(dotenv.config).not.toHaveBeenCalled();
    });
  });
});
