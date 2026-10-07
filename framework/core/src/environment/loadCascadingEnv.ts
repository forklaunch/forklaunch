import dotenv from 'dotenv';
import { existsSync } from 'fs';
import { dirname, resolve } from 'path';

/**
 * Gets cascading environment file paths: collects all .env.local files from project directory up to root
 * Root detection uses: .forklaunch/manifest.toml
 */
export function getCascadingEnvPaths(
  projectEnvPath: string | undefined,
  projectRoot: string = process.cwd()
): {
  rootEnvExists: boolean;
  projectEnvExists: boolean;
  rootEnvPath?: string;
  projectEnvFilePath?: string;
  loadOrder: string[];
} {
  const result = {
    rootEnvExists: false,
    projectEnvExists: false,
    rootEnvPath: undefined as string | undefined,
    projectEnvFilePath: undefined as string | undefined,
    loadOrder: [] as string[]
  };

  const applicationRoot = findApplicationRoot(projectRoot);

  const envLocalFiles = collectEnvLocalFiles(projectRoot, applicationRoot);
  result.loadOrder.push(...envLocalFiles);

  const rootEnvPath = resolve(applicationRoot, '.env.local');
  if (envLocalFiles.includes(rootEnvPath)) {
    result.rootEnvExists = true;
    result.rootEnvPath = rootEnvPath;
  }

  if (projectEnvPath) {
    const fullProjectEnvPath = resolve(projectRoot, projectEnvPath);
    if (existsSync(fullProjectEnvPath)) {
      result.projectEnvExists = true;
      result.projectEnvFilePath = fullProjectEnvPath;
      if (!result.loadOrder.includes(fullProjectEnvPath)) {
        result.loadOrder.push(fullProjectEnvPath);
      }
    }
  }

  return result;
}

/**
 * Loads environment variables with cascading precedence: all .env.local files from root to project, then project env file
 */
export function loadCascadingEnv(
  projectEnvPath: string | undefined,
  projectRoot: string = process.cwd()
): {
  rootEnvLoaded: boolean;
  projectEnvLoaded: boolean;
  rootEnvPath?: string;
  projectEnvFilePath?: string;
  envFilesLoaded: string[];
  totalEnvFilesLoaded: number;
} {
  const paths = getCascadingEnvPaths(projectEnvPath, projectRoot);
  const result = {
    rootEnvLoaded: false,
    projectEnvLoaded: false,
    rootEnvPath: paths.rootEnvPath,
    projectEnvFilePath: paths.projectEnvFilePath,
    envFilesLoaded: [] as string[],
    totalEnvFilesLoaded: 0
  };

  const seen = new Set<string>();
  for (const envPath of paths.loadOrder) {
    if (seen.has(envPath)) {
      continue;
    }
    seen.add(envPath);

    const envResult = dotenv.config({
      path: envPath,
      override: true
    });

    if (!envResult?.error) {
      result.envFilesLoaded.push(envPath);
      result.totalEnvFilesLoaded++;

      if (envPath === paths.rootEnvPath) {
        result.rootEnvLoaded = true;
      }
      if (envPath === paths.projectEnvFilePath) {
        result.projectEnvLoaded = true;
      }
    }
  }

  return result;
}

/**
 * Collects all .env.local files from project directory up to application root
 * Returns paths in order from root to project (for proper precedence)
 */
function collectEnvLocalFiles(
  projectRoot: string,
  applicationRoot: string
): string[] {
  const envLocalPaths: string[] = [];

  let currentPath = resolve(projectRoot);
  const normalizedAppRoot = resolve(applicationRoot);

  while (currentPath.length >= normalizedAppRoot.length) {
    const envLocalPath = resolve(currentPath, '.env.local');
    if (existsSync(envLocalPath)) {
      envLocalPaths.push(envLocalPath);
    }

    if (currentPath === normalizedAppRoot) {
      break;
    }

    const parentPath = dirname(currentPath);
    if (parentPath === currentPath) {
      break;
    }

    currentPath = parentPath;
  }

  return envLocalPaths.reverse();
}

/**
 * Finds application root by looking for .forklaunch/manifest.toml
 */
export function findApplicationRoot(startPath: string): string {
  let currentPath = resolve(startPath);
  const originalStart = currentPath;
  const maxDepth = 10;
  let depth = 0;

  while (depth < maxDepth) {
    const forklaunchManifest = resolve(
      currentPath,
      '.forklaunch',
      'manifest.toml'
    );
    if (existsSync(forklaunchManifest)) {
      return currentPath;
    }

    const parentPath = dirname(currentPath);
    if (parentPath === currentPath) {
      break;
    }

    currentPath = parentPath;
    depth++;
  }

  return originalStart;
}
