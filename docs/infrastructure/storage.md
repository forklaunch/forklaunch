---
title: "Object Storage"
description: "S3-compatible object storage for file uploads, document management, and binary data."
category: "Infrastructure"
---

## Overview

ForkLaunch provides S3-compatible object storage through `@forklaunch/infrastructure-s3`. Locally, it uses MinIO (an S3-compatible server) in Docker. In production, it connects to Amazon S3.

Use object storage for large files, user uploads, documents, images, and any binary data that doesn't belong in a database.

## Quick Start

```typescript
import { S3ObjectStore } from '@forklaunch/infrastructure-s3';

const objectStore = new S3ObjectStore(openTelemetryCollector, {
  endpoint: process.env.S3_ENDPOINT,
  region: process.env.S3_REGION,
  credentials: {
    accessKeyId: process.env.S3_ACCESS_KEY_ID,
    secretAccessKey: process.env.S3_SECRET_ACCESS_KEY
  },
  forcePathStyle: true // Required for MinIO in development
});
```

## Core Operations

### Upload a File

```typescript
await objectStore.putObject({
  bucket: 'uploads',
  key: `users/${userId}/avatar.png`,
  body: fileBuffer,
  contentType: 'image/png'
});
```

### Download a File

```typescript
const result = await objectStore.getObject({
  bucket: 'uploads',
  key: `users/${userId}/avatar.png`
});

// result.body is a readable stream
const buffer = await streamToBuffer(result.body);
```

### Delete a File

```typescript
await objectStore.deleteObject({
  bucket: 'uploads',
  key: `users/${userId}/avatar.png`
});
```

### List Files

```typescript
const files = await objectStore.listObjects({
  bucket: 'uploads',
  prefix: `users/${userId}/`
});

for (const file of files) {
  console.log(file.key, file.size, file.lastModified);
}
```

### Check if a File Exists

```typescript
const exists = await objectStore.headObject({
  bucket: 'uploads',
  key: `users/${userId}/avatar.png`
});
```

## Common Patterns

### User File Uploads

```typescript
async function uploadUserFile(
  params: { em: EntityManager; userId: string; file: Buffer; filename: string }
) {
  const { userId, file, filename } = params;
  const key = `users/${userId}/files/${Date.now()}-${filename}`;

  await objectStore.putObject({
    bucket: 'uploads',
    key,
    body: file,
    contentType: getMimeType(filename)
  });

  return { key, url: `/${key}` };
}
```

### Document Versioning

```typescript
async function uploadDocumentVersion(
  documentId: string,
  version: number,
  content: Buffer
) {
  const key = `documents/${documentId}/v${version}`;

  await objectStore.putObject({
    bucket: 'documents',
    key,
    body: content,
    contentType: 'application/pdf'
  });
}

async function getLatestVersion(documentId: string) {
  const versions = await objectStore.listObjects({
    bucket: 'documents',
    prefix: `documents/${documentId}/`
  });

  const latest = versions.sort((a, b) => b.key.localeCompare(a.key))[0];
  return objectStore.getObject({ bucket: 'documents', key: latest.key });
}
```

### Streaming Large Files

```typescript
async function streamFile(res: Response, bucket: string, key: string) {
  const result = await objectStore.getObject({ bucket, key });
  result.body.pipe(res);
}
```

## Key Organization

Organize object keys with clear prefixes:

```
uploads/
  users/{userId}/
    avatar.png
    files/
      1234567890-document.pdf
  organizations/{orgId}/
    logo.png

deployments/
  {deploymentId}/
    logs/
      build.log
      deploy.log
    artifacts/
      bundle.tar.gz
```

## Environment Variables

```bash
# Local (MinIO)
S3_ENDPOINT=http://localhost:9000
S3_ACCESS_KEY_ID=minioadmin
S3_SECRET_ACCESS_KEY=minioadmin
S3_REGION=us-east-1
S3_BUCKET=my-bucket

# Production (AWS S3)
S3_ACCESS_KEY_ID=AKIA...
S3_SECRET_ACCESS_KEY=...
S3_REGION=us-east-1
S3_BUCKET=my-production-bucket
# No S3_ENDPOINT needed; defaults to AWS
```

## Testing

Use `BlueprintTestHarness` with MinIO containers:

```typescript
import { BlueprintTestHarness } from '@forklaunch/testing';

const harness = new BlueprintTestHarness({
  needsS3: true,
  s3Bucket: 'test-uploads'
});

const setup = await harness.setup();
// S3 endpoint available via process.env.S3_ENDPOINT
```

## Best Practices

1. **Use meaningful key prefixes**: organize by entity type and ID
2. **Set content types**: helps with browser rendering and downloads
3. **Don't store small data in S3**: use the cache or database instead
4. **Stream large files**: don't load entire files into memory
5. **Use `forcePathStyle: true`** in development for MinIO compatibility

## Related Documentation

- [Infrastructure Overview](/docs/infrastructure/overview.md)
- [Caches](/docs/infrastructure/caches.md)
- [Testing Guide](/docs/guides/testing.md): Testing with MinIO containers
