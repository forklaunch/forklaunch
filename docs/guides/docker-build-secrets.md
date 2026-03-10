---
title: Docker Build Secrets
category: Advanced Guides
description: How to securely mount private registry credentials during Docker builds using BuildKit secrets.
---

# Docker Build Secrets for Private Packages

ForkLaunch uses `@pulumi/docker-build` (BuildKit-native) for all Docker image builds during deployment. This enables secure secret mounting for private package registries.

## How It Works

During deployment, ForkLaunch can pass BuildKit secrets to your Docker builds. These secrets are mounted at build time but **never baked into image layers**, keeping your credentials secure.

## Using Private npm/pnpm Packages

### 1. Configure your npm token

Set your registry auth token as an environment variable or application secret in the ForkLaunch dashboard. The token will be passed as a BuildKit secret named `npmrc`.

### 2. Update your Dockerfile

Use `--mount=type=secret` in your `RUN` instructions:

```dockerfile
# For pnpm with private registry
FROM node:20-slim AS deps
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc \
    pnpm install --frozen-lockfile

# For npm with private registry
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc \
    npm ci

# For bun with private registry
RUN --mount=type=secret,id=npmrc,target=/root/.bunfig.toml \
    bun install --frozen-lockfile
```

### 3. Example .npmrc format

Your npm token should produce an `.npmrc` like:

```
//registry.npmjs.org/:_authToken=${NPM_TOKEN}
@your-scope:registry=https://npm.pkg.github.com
//npm.pkg.github.com/:_authToken=${GITHUB_TOKEN}
```

## Security Notes

- Secrets are mounted via BuildKit's `--mount=type=secret` mechanism
- They exist only during the build step that mounts them
- They are **never** included in the final image layers
- They are **never** visible in `docker history` or image inspection
- Each build gets a fresh secret mount; tokens are not cached between builds

## Build Caching

ForkLaunch uses registry-based BuildKit caching with `mode=max`, which caches all intermediate layers (including dependency install layers). This means:

- First build: full `pnpm install` / `npm ci` / `bun install`
- Subsequent builds with unchanged lockfile: cached (seconds instead of minutes)
- Cache is shared across deployments and across apps using the same ECR repository
