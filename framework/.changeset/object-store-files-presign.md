---
'@forklaunch/core': minor
'@forklaunch/infrastructure-s3': minor
---

Object store: real files, browser uploads and download links, keyless on ForkLaunch.

- `ObjectStore` gains `putFile(key, body, { contentType, filename?, metadata? })`,
  `presignUpload(key, { contentType, maxBytes, expiresIn? })` (a presigned POST that
  enforces size and content type) and `presignDownload(key, { expiresIn?, filename? })`.
  Custom `ObjectStore` implementations must add them.
- `S3ObjectStore` takes `prefix` (confines every key), `presignLimits` (lifetime caps,
  from `S3_PRESIGN_MAX_UPLOAD_SECONDS` / `S3_PRESIGN_MAX_DOWNLOAD_SECONDS`) and
  `createBucketIfMissing`, which now defaults to true only when a custom endpoint
  (MinIO) is configured: deployed buckets are provisioned by the platform.
- `s3ClientConfig({ url, region, accessKeyId, secretAccessKey })` passes keys only when
  both are set, so deployed services use their task role.
