import { existsSync, mkdirSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { afterEach, beforeEach, describe, expect, test } from 'vitest';
import {
  getCascadingEnvPaths,
  loadCascadingEnv
} from '../src/environment/loadCascadingEnv';

describe('loadCascadingEnv', () => {
  let tempDir: string;
  let projectDir: string;
  let rootDir: string;

  beforeEach(() => {
    tempDir = join(tmpdir(), `forklaunch-test-${Date.now()}`);
    rootDir = join(tempDir, 'root');
    projectDir = join(rootDir, 'src', 'project');

    mkdirSync(projectDir, { recursive: true });
  });

  afterEach(() => {
    if (existsSync(tempDir)) {
      rmSync(tempDir, { recursive: true });
    }
  });

  describe('getCascadingEnvPaths', () => {
    test('should return empty result when no env files exist', () => {
      const result = getCascadingEnvPaths(undefined, projectDir);

      expect(result.rootEnvExists).toBe(false);
      expect(result.projectEnvExists).toBe(false);
      expect(result.loadOrder).toEqual([]);
    });

    test('should find root .env.local when it exists', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');

      const result = getCascadingEnvPaths(undefined, projectDir);

      expect(result.rootEnvExists).toBe(true);
      expect(result.rootEnvPath).toBe(join(rootDir, '.env.local'));
      expect(result.loadOrder).toEqual([join(rootDir, '.env.local')]);
    });

    test('should find both root and project env files', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(join(projectDir, '.env'), 'PROJECT_VAR=project_value');

      const result = getCascadingEnvPaths('.env', projectDir);

      expect(result.rootEnvExists).toBe(true);
      expect(result.projectEnvExists).toBe(true);
      expect(result.loadOrder).toEqual([
        join(rootDir, '.env.local'),
        join(projectDir, '.env')
      ]);
    });

    test('should collect multiple .env.local files in directory hierarchy', () => {
      const intermediateDir = join(rootDir, 'src');

      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      // Create .env.local files at different levels
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(
        join(intermediateDir, '.env.local'),
        'INTERMEDIATE_VAR=intermediate_value'
      );
      writeFileSync(
        join(projectDir, '.env.local'),
        'PROJECT_VAR=project_value'
      );

      const result = getCascadingEnvPaths(undefined, projectDir);

      expect(result.loadOrder).toEqual([
        join(rootDir, '.env.local'),
        join(intermediateDir, '.env.local'),
        join(projectDir, '.env.local')
      ]);
    });

    test('should handle partial .env.local file hierarchy', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      // Only create .env.local at root and project level (skip intermediate)
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(
        join(projectDir, '.env.local'),
        'PROJECT_VAR=project_value'
      );

      const result = getCascadingEnvPaths(undefined, projectDir);

      expect(result.loadOrder).toEqual([
        join(rootDir, '.env.local'),
        join(projectDir, '.env.local')
      ]);
    });

    test('should combine .env.local files with project env file', () => {
      const intermediateDir = join(rootDir, 'src');

      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(
        join(intermediateDir, '.env.local'),
        'INTERMEDIATE_VAR=intermediate_value'
      );
      writeFileSync(join(projectDir, '.env'), 'PROJECT_VAR=project_value');

      const result = getCascadingEnvPaths('.env', projectDir);

      expect(result.loadOrder).toEqual([
        join(rootDir, '.env.local'),
        join(intermediateDir, '.env.local'),
        join(projectDir, '.env')
      ]);
    });
  });

  describe('loadCascadingEnv', () => {
    test('should load root .env.local successfully', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');

      const result = loadCascadingEnv(undefined, projectDir);

      expect(result.rootEnvLoaded).toBe(true);
      expect(result.projectEnvLoaded).toBe(false);
      expect(result.rootEnvPath).toBe(join(rootDir, '.env.local'));
      expect(result.envFilesLoaded).toEqual([join(rootDir, '.env.local')]);
      expect(result.totalEnvFilesLoaded).toBe(1);
      expect(process.env.ROOT_VAR).toBe('root_value');
    });

    test('should load both root and project env files', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(join(projectDir, '.env'), 'PROJECT_VAR=project_value');

      const result = loadCascadingEnv('.env', projectDir);

      expect(result.rootEnvLoaded).toBe(true);
      expect(result.projectEnvLoaded).toBe(true);
      expect(result.totalEnvFilesLoaded).toBe(2);
      expect(process.env.ROOT_VAR).toBe('root_value');
      expect(process.env.PROJECT_VAR).toBe('project_value');
    });

    test('should load multiple .env.local files with proper precedence', () => {
      const intermediateDir = join(rootDir, 'src');

      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      // Create .env.local files with overlapping variables
      writeFileSync(
        join(rootDir, '.env.local'),
        'SHARED_VAR=root_value\nROOT_ONLY=root_only'
      );
      writeFileSync(
        join(intermediateDir, '.env.local'),
        'SHARED_VAR=intermediate_value\nINTERMEDIATE_ONLY=intermediate_only'
      );
      writeFileSync(
        join(projectDir, '.env.local'),
        'SHARED_VAR=project_value\nPROJECT_ONLY=project_only'
      );

      const result = loadCascadingEnv(undefined, projectDir);

      expect(result.totalEnvFilesLoaded).toBe(3);
      expect(result.envFilesLoaded).toEqual([
        join(rootDir, '.env.local'),
        join(intermediateDir, '.env.local'),
        join(projectDir, '.env.local')
      ]);

      // Check precedence: project .env.local should override intermediate and root
      expect(process.env.SHARED_VAR).toBe('project_value');
      expect(process.env.ROOT_ONLY).toBe('root_only');
      expect(process.env.INTERMEDIATE_ONLY).toBe('intermediate_only');
      expect(process.env.PROJECT_ONLY).toBe('project_only');
    });

    test('should handle project env override correctly', () => {
      const intermediateDir = join(rootDir, 'src');

      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      writeFileSync(join(rootDir, '.env.local'), 'SHARED_VAR=root_value');
      writeFileSync(
        join(intermediateDir, '.env.local'),
        'SHARED_VAR=intermediate_value'
      );
      writeFileSync(join(projectDir, '.env'), 'SHARED_VAR=project_env_value');

      const result = loadCascadingEnv('.env', projectDir);

      expect(result.rootEnvLoaded).toBe(true);
      expect(result.projectEnvLoaded).toBe(true);
      expect(result.totalEnvFilesLoaded).toBe(3);
      // Project .env should have highest precedence
      expect(process.env.SHARED_VAR).toBe('project_env_value');
    });

    test('should handle partial .env.local hierarchy gracefully', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      // Only create .env.local at root and project level (skip intermediate)
      writeFileSync(join(rootDir, '.env.local'), 'ROOT_VAR=root_value');
      writeFileSync(
        join(projectDir, '.env.local'),
        'PROJECT_VAR=project_value'
      );

      const result = loadCascadingEnv(undefined, projectDir);

      expect(result.totalEnvFilesLoaded).toBe(2);
      expect(result.envFilesLoaded).toEqual([
        join(rootDir, '.env.local'),
        join(projectDir, '.env.local')
      ]);
      expect(process.env.ROOT_VAR).toBe('root_value');
      expect(process.env.PROJECT_VAR).toBe('project_value');
    });

    test('should handle missing env files gracefully', () => {
      mkdirSync(join(rootDir, '.forklaunch'), { recursive: true });
      writeFileSync(
        join(rootDir, '.forklaunch', 'manifest.toml'),
        'test = "value"'
      );

      const result = loadCascadingEnv('.env', projectDir);

      expect(result.rootEnvLoaded).toBe(false);
      expect(result.projectEnvLoaded).toBe(false);
      expect(result.totalEnvFilesLoaded).toBe(0);
      expect(result.envFilesLoaded).toEqual([]);
    });
  });
});
