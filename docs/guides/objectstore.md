---
title: Object Store Module
category: Framework
description: Complete guide to using the Object Store system for large file and binary storage with S3.
---

## Overview

ForkLaunch provides an Object Store abstraction through `@forklaunch/core/objectstore` with a production-ready AWS S3 implementation in `@forklaunch/infrastructure-s3`. The Object Store is designed for storing and retrieving large files, binary data, and structured JSON objects with streaming support.

## Quick Start

### Installation

```bash
# Core object store interface
pnpm add @forklaunch/core

# S3 implementation
pnpm add @forklaunch/infrastructure-s3 @aws-sdk/client-s3
```

### Basic Usage

```typescript
import { S3ObjectStore } from '@forklaunch/infrastructure-s3';
import { OpenTelemetryCollector } from '@forklaunch/core/http';

// Create an object store instance
const store = new S3ObjectStore(
  openTelemetryCollector,
  {
    bucket: 'my-app-storage',
    clientConfig: {
      region: 'us-west-2',
      credentials: {
        accessKeyId: process.env.AWS_ACCESS_KEY_ID!,
        secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY!
      }
    }
  },
  {
    enabled: {
      logging: true,
      metrics: true,
      tracing: true
    }
  }
);

// Store an object
await store.putObject({
  key: 'users/123/profile',
  name: 'Alice',
  email: 'alice@example.com',
  avatar: 'https://...'
});

// Retrieve an object
const profile = await store.readObject<UserProfile>('users/123/profile');
console.log(profile); // { name: 'Alice', email: 'alice@example.com', ... }

// Delete an object
await store.deleteObject('users/123/profile');
```

## Core Concepts

### Object Keys

Object keys are unique identifiers for stored objects. They work like file paths:

```typescript
'users/123/profile'           // User profile
'documents/2024/report.pdf'   // Document with path structure
'images/thumbnails/abc123'    // Image thumbnail
'logs/2024-01-15/app.log'     // Date-based logs
```

### ObjectStoreKey Utility

Generate consistent object keys with prefixes:

```typescript
import { createObjectStoreKey } from '@forklaunch/core/objectstore';

const createUserKey = createObjectStoreKey('user');
const createDocKey = createObjectStoreKey('document');

const profileKey = createUserKey('123');           // 'user-123'
const docKey = createDocKey(['2024', 'report']);   // 'document-2024-report'
```

### Automatic Bucket Creation

The S3ObjectStore automatically creates the bucket on first use if it doesn't exist:

```typescript
const store = new S3ObjectStore(otelCollector, { bucket: 'new-bucket' }, telemetryOptions);

// Bucket is created automatically on first operation
await store.putObject({ key: 'test', value: 'hello' });
```

## API Reference

### Basic Operations

#### putObject

Store a JSON-serializable object. The object must include a `key` property.

```typescript
interface Document {
  key: string;
  title: string;
  content: string;
  createdAt: string;
}

await store.putObject<Document>({
  key: 'docs/welcome',
  title: 'Welcome',
  content: 'Welcome to our app!',
  createdAt: new Date().toISOString()
});
```

**Note**: The `key` property is extracted and used as the S3 object key. The remaining properties are stored as JSON.

#### readObject

Retrieve an object by its key.

```typescript
const doc = await store.readObject<Document>('docs/welcome');
console.log(doc.title);    // 'Welcome'
console.log(doc.content);  // 'Welcome to our app!'
// Note: 'key' is not included in the returned object
```

**Throws**: Error if the object doesn't exist or S3 doesn't return a body.

#### deleteObject

Remove an object from storage.

```typescript
await store.deleteObject('docs/welcome');
```

### Batch Operations

Process multiple objects in parallel for better performance.

#### putBatchObjects

Store multiple objects at once:

```typescript
await store.putBatchObjects([
  { key: 'users/1', name: 'Alice', email: 'alice@example.com' },
  { key: 'users/2', name: 'Bob', email: 'bob@example.com' },
  { key: 'users/3', name: 'Charlie', email: 'charlie@example.com' }
]);
```

#### readBatchObjects

Read multiple objects by their keys:

```typescript
const users = await store.readBatchObjects<User>([
  'users/1',
  'users/2',
  'users/3'
]);

console.log(users.length); // 3
```

**Note**: If any object doesn't exist, the entire operation fails. Handle errors appropriately.

#### deleteBatchObjects

Delete multiple objects in a single S3 request:

```typescript
await store.deleteBatchObjects([
  'users/1',
  'users/2',
  'users/3'
]);
```

### Streaming Operations

For large files or when you need stream-based processing.

#### streamDownloadObject

Download an object as a Node.js Readable stream:

```typescript
import * as fs from 'fs';

const stream = await store.streamDownloadObject('large-file.json');

// Pipe to file
stream.pipe(fs.createWriteStream('downloaded.json'));

// Or process chunks
stream.on('data', (chunk) => {
  console.log('Received chunk:', chunk.length, 'bytes');
});

stream.on('end', () => {
  console.log('Download complete');
});

stream.on('error', (err) => {
  console.error('Download failed:', err);
});
```

#### streamDownloadBatchObjects

Download multiple objects as streams:

```typescript
const streams = await store.streamDownloadBatchObjects([
  'file1.json',
  'file2.json',
  'file3.json'
]);

streams[0].pipe(fs.createWriteStream('file1.json'));
streams[1].pipe(fs.createWriteStream('file2.json'));
streams[2].pipe(fs.createWriteStream('file3.json'));
```

#### streamUploadObject

Upload an object (currently uses `putObject` internally for compatibility):

```typescript
await store.streamUploadObject({
  key: 'uploads/large-document',
  content: largeContent
});
```

#### streamUploadBatchObjects

Upload multiple objects:

```typescript
await store.streamUploadBatchObjects([
  { key: 'upload1', data: data1 },
  { key: 'upload2', data: data2 }
]);
```

### Utility Methods

#### getClient

Access the underlying S3 client for advanced operations:

```typescript
const s3Client = store.getClient();

// Use S3 SDK directly
import { ListObjectsV2Command } from '@aws-sdk/client-s3';

const result = await s3Client.send(
  new ListObjectsV2Command({
    Bucket: 'my-bucket',
    Prefix: 'users/'
  })
);
```

## Common Patterns

### User File Storage

```typescript
import { createObjectStoreKey } from '@forklaunch/core/objectstore';

const createUserFileKey = createObjectStoreKey('user-files');

// Upload user avatar
async function uploadAvatar(userId: string, imageData: Buffer) {
  const key = createUserFileKey([userId, 'avatar']);

  await store.putObject({
    key,
    contentType: 'image/jpeg',
    data: imageData.toString('base64'),
    uploadedAt: new Date().toISOString()
  });

  return key;
}

// Download user avatar
async function getAvatar(userId: string): Promise<Buffer> {
  const key = createUserFileKey([userId, 'avatar']);
  const stream = await store.streamDownloadObject(key);

  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    stream.on('data', (chunk) => chunks.push(chunk));
    stream.on('end', () => resolve(Buffer.concat(chunks)));
    stream.on('error', reject);
  });
}
```

### Document Versioning

```typescript
interface DocumentVersion {
  key: string;
  version: number;
  title: string;
  content: string;
  createdAt: string;
  authorId: string;
}

async function saveDocumentVersion(
  docId: string,
  version: number,
  data: Omit<DocumentVersion, 'key' | 'version'>
) {
  await store.putObject<DocumentVersion>({
    key: `documents/${docId}/v${version}`,
    version,
    ...data
  });
}

async function getDocumentHistory(docId: string): Promise<DocumentVersion[]> {
  // Get all versions (requires listing support)
  const s3Client = store.getClient();
  const { Contents } = await s3Client.send(
    new ListObjectsV2Command({
      Bucket: 'my-bucket',
      Prefix: `documents/${docId}/`
    })
  );

  if (!Contents) return [];

  const keys = Contents.map(obj => obj.Key!);
  return await store.readBatchObjects<DocumentVersion>(keys);
}
```

### Backup and Export

```typescript
async function exportUserData(userId: string): Promise<NodeJS.ReadableStream> {
  const { PassThrough } = require('stream');
  const exportStream = new PassThrough();

  // Collect all user data
  const [profile, settings, documents] = await Promise.all([
    store.readObject(`users/${userId}/profile`),
    store.readObject(`users/${userId}/settings`),
    store.readBatchObjects([
      `users/${userId}/doc1`,
      `users/${userId}/doc2`
    ])
  ]);

  // Write to stream as JSON
  exportStream.write(JSON.stringify({
    profile,
    settings,
    documents
  }, null, 2));

  exportStream.end();
  return exportStream;
}
```

### Temporary File Storage

```typescript
interface TempFile {
  key: string;
  data: any;
  expiresAt: string;
}

async function storeTempFile(fileId: string, data: any, ttlSeconds: number) {
  const expiresAt = new Date(Date.now() + ttlSeconds * 1000).toISOString();

  await store.putObject<TempFile>({
    key: `temp/${fileId}`,
    data,
    expiresAt
  });

  return fileId;
}

async function getTempFile(fileId: string): Promise<any | null> {
  try {
    const file = await store.readObject<TempFile>(`temp/${fileId}`);

    // Check if expired
    if (new Date(file.expiresAt) < new Date()) {
      await store.deleteObject(`temp/${fileId}`);
      return null;
    }

    return file.data;
  } catch {
    return null;
  }
}

// Cleanup job
async function cleanupExpiredFiles() {
  const s3Client = store.getClient();
  const { Contents } = await s3Client.send(
    new ListObjectsV2Command({
      Bucket: 'my-bucket',
      Prefix: 'temp/'
    })
  );

  if (!Contents) return;

  const expiredKeys: string[] = [];

  for (const obj of Contents) {
    const file = await store.readObject<TempFile>(obj.Key!);
    if (new Date(file.expiresAt) < new Date()) {
      expiredKeys.push(obj.Key!);
    }
  }

  if (expiredKeys.length > 0) {
    await store.deleteBatchObjects(expiredKeys);
  }
}
```

### Large File Uploads with Progress

```typescript
async function uploadLargeFile(
  key: string,
  fileStream: NodeJS.ReadableStream
): Promise<void> {
  const chunks: Buffer[] = [];
  let totalSize = 0;

  for await (const chunk of fileStream) {
    chunks.push(chunk);
    totalSize += chunk.length;
    console.log(`Uploaded ${totalSize} bytes...`);
  }

  const completeData = Buffer.concat(chunks);

  await store.putObject({
    key,
    data: completeData.toString('base64'),
    size: totalSize,
    uploadedAt: new Date().toISOString()
  });
}
```

## S3-Specific Configuration

### AWS Credentials

```typescript
// Option 1: Explicit credentials
const store = new S3ObjectStore(
  otelCollector,
  {
    bucket: 'my-bucket',
    clientConfig: {
      region: 'us-west-2',
      credentials: {
        accessKeyId: process.env.AWS_ACCESS_KEY_ID!,
        secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY!
      }
    }
  },
  telemetryOptions
);

// Option 2: IAM role (for EC2/ECS/Lambda)
const store = new S3ObjectStore(
  otelCollector,
  {
    bucket: 'my-bucket',
    clientConfig: {
      region: 'us-west-2'
      // Credentials automatically loaded from IAM role
    }
  },
  telemetryOptions
);

// Option 3: Use existing S3 client
import { S3Client } from '@aws-sdk/client-s3';

const s3Client = new S3Client({ region: 'us-west-2' });
const store = new S3ObjectStore(
  otelCollector,
  {
    bucket: 'my-bucket',
    client: s3Client
  },
  telemetryOptions
);
```

### Bucket Lifecycle Policies

Configure S3 bucket lifecycle rules for automatic cleanup:

```typescript
// This is done via AWS CLI or Console, not in application code
// Example AWS CLI command:
// aws s3api put-bucket-lifecycle-configuration \
//   --bucket my-bucket \
//   --lifecycle-configuration file://lifecycle.json
```

Example `lifecycle.json`:
```json
{
  "Rules": [
    {
      "Id": "DeleteTempFilesAfter7Days",
      "Status": "Enabled",
      "Prefix": "temp/",
      "Expiration": {
        "Days": 7
      }
    }
  ]
}
```

## Best Practices

### 1. Use Descriptive Key Structures

```typescript
// ✅ Good - hierarchical, organized
'users/123/profile'
'users/123/documents/report-2024.pdf'
'images/products/thumb_456.jpg'

// ❌ Avoid - flat, unorganized
'user_123_profile'
'doc_report_2024'
'img_456'
```

### 2. Handle Errors Gracefully

```typescript
async function getDocument(key: string): Promise<Document | null> {
  try {
    return await store.readObject<Document>(key);
  } catch (error) {
    if (error.name === 'NoSuchKey') {
      return null; // Document doesn't exist
    }
    throw error; // Unexpected error
  }
}
```

### 3. Use Batch Operations When Possible

```typescript
// ✅ Good - single batch operation
await store.deleteBatchObjects(['file1', 'file2', 'file3']);

// ❌ Avoid - multiple sequential operations
await store.deleteObject('file1');
await store.deleteObject('file2');
await store.deleteObject('file3');
```

### 4. Stream Large Files

```typescript
// ✅ Good - streams large files
const stream = await store.streamDownloadObject('large-video.mp4');
stream.pipe(response);

// ❌ Avoid - loads entire file into memory
const video = await store.readObject('large-video.mp4');
response.send(video);
```

### 5. Use Type Safety

```typescript
interface UserProfile {
  name: string;
  email: string;
  avatarUrl?: string;
}

// ✅ Type-safe operations
const profile = await store.readObject<UserProfile>('users/123/profile');
const name: string = profile.name; // TypeScript validates

// ❌ Avoid - untyped
const profile = await store.readObject('users/123/profile');
const name = profile.name; // any type
```

### 6. Organize Keys with Prefixes

```typescript
const createUserKey = createObjectStoreKey('user');
const createOrgKey = createObjectStoreKey('org');
const createDocKey = createObjectStoreKey('doc');

// All user data under 'user-*'
await store.putObject({ key: createUserKey('123'), ...userData });

// All org data under 'org-*'
await store.putObject({ key: createOrgKey('456'), ...orgData });

// Easy to list/cleanup by prefix
```

## Performance Optimization

### 1. Parallel Uploads

```typescript
// Upload multiple files in parallel
const uploads = files.map(file =>
  store.putObject({
    key: `uploads/${file.id}`,
    ...file.data
  })
);

await Promise.all(uploads);
```

### 2. Multipart Uploads for Large Files

For files > 5MB, consider using S3 multipart upload via the S3 SDK directly:

```typescript
import { Upload } from '@aws-sdk/lib-storage';

const s3Client = store.getClient();

const upload = new Upload({
  client: s3Client,
  params: {
    Bucket: 'my-bucket',
    Key: 'large-file.zip',
    Body: fileStream
  }
});

upload.on('httpUploadProgress', (progress) => {
  console.log(`Uploaded ${progress.loaded} / ${progress.total} bytes`);
});

await upload.done();
```

### 3. Use CloudFront for Static Assets

For frequently accessed files, use CloudFront CDN:

```typescript
const CDN_URL = 'https://d123456.cloudfront.net';

async function getPublicFileUrl(key: string): Promise<string> {
  return `${CDN_URL}/${key}`;
}
```

## Testing

### Using LocalStack

```typescript
import { TestContainerManager } from '@forklaunch/testing';

const manager = new TestContainerManager();
const localstack = await manager.startLocalStack();

const store = new S3ObjectStore(
  otelCollector,
  {
    bucket: 'test-bucket',
    clientConfig: {
      region: 'us-east-1',
      endpoint: localstack.getS3Endpoint(),
      credentials: {
        accessKeyId: 'test',
        secretAccessKey: 'test'
      },
      forcePathStyle: true
    }
  },
  telemetryOptions
);

// Run tests...

await manager.stopAll();
```

### Mock Implementation

```typescript
import { ObjectStore } from '@forklaunch/core/objectstore';

class InMemoryObjectStore implements ObjectStore<Map<string, any>> {
  private store = new Map<string, any>();

  async putObject<T>(object: T & { key: string }): Promise<void> {
    const { key, ...rest } = object;
    this.store.set(key, rest);
  }

  async readObject<T>(key: string): Promise<T> {
    const obj = this.store.get(key);
    if (!obj) throw new Error('Object not found');
    return obj;
  }

  async deleteObject(key: string): Promise<void> {
    this.store.delete(key);
  }

  // ... implement other methods
}

// Use in tests
const store = new InMemoryObjectStore();
```

## Comparison with Cache

| Feature | Object Store | Cache |
|---------|-------------|-------|
| **Purpose** | Long-term file/document storage | Short-term data caching |
| **Size Limit** | GB to TB per object | KB to MB per record |
| **TTL** | No automatic expiration | Required TTL |
| **Cost** | Higher storage cost | Lower (in-memory) |
| **Access Speed** | Slower (network I/O) | Faster (in-memory) |
| **Use Cases** | Files, images, backups | Session data, API responses |
| **Streaming** | Full streaming support | Limited streaming |

## Related Documentation

- [Cache](/docs/guides/cache.md): For short-term data caching
- [Testing](/docs/guides/testing.md): TestContainers setup for S3
- [Telemetry](/docs/framework/telemetry.md): Monitoring object store performance
- [HTTP Framework](/docs/framework/http.md): Integrating with HTTP endpoints
