---
title: Infra Command
category: CLI Reference
description: Add or remove a capability in a service's or worker's code with forklaunch infra add|remove.
---

## Overview

`forklaunch infra add|remove` gives an existing service (or worker) a capability and takes it away again. It edits the code in place: the registration in `registrations.ts`, the local stand-in in `docker-compose.yaml`, `.env.local`, `package.json`, the manifest resource and, for a capability with events, the `/platform-events` route. Run `forklaunch score --offline` afterwards; its wiring checks should pass.

The other `forklaunch infra` subcommands (`list`, `status`, `resize`, `config-set`, `stop`, `delete`) inspect and manage resources the platform has already provisioned. They never write code.

## Usage

```bash
forklaunch infra add <service> <capability> [options]
forklaunch infra remove <service> <capability> [options]
```

**Arguments:**
- `<service>` - The project to change, by name: a service, or a worker for a managed gateway capability
- `<capability>` - What to add or remove (table below)

**Options:**
- `-p, --path <path>` - Application root (defaults to the manifest in the current directory)
- `-n, --dryrun` - Show what would change without writing

## Capabilities

| Capability | What the service gets | Release resource type | On a worker | Events |
| :--------- | :-------------------- | :-------------------- | :---------- | :----- |
| `object-store` (`s3`) | an S3 object store (MinIO locally); manifest `object_store` | - | no (use `forklaunch change worker`) | - |
| `cache` (`redis`) | a Redis cache; manifest `cache` | - | no (use `forklaunch change worker`) | - |
| `email` | `EmailClient` (`createEmailClient`) | `email` | yes | delivered, bounced, complained |
| `sms` | `SmsClient` (`createSmsClient`) | `sms` | yes | delivered, failed, received, opted out |
| `whatsapp` | `WhatsAppClient` (`createWhatsAppClient`) | `whatsapp` | yes | received, status |
| `voice` | `VoiceClient` (`createVoiceClient`) | `voice` | yes | call started, call ended, recording ready |
| `payments` | `StripeClient` (`createStripeClient`) | `payment` | yes | Stripe events (`checkout.session.completed`, ...) |
| `models` | `ModelGatewayClient` (`createModelGatewayClient`) | `models` | yes | none |

The last six are **managed gateway** capabilities: the platform holds the vendor account (SES, AWS End User Messaging, Amazon Connect, Stripe Connect, Azure AI Foundry) and the app holds no vendor key. They work for any app deployed on ForkLaunch, whether it is a managed instance or an app deployed with `forklaunch deploy create`. See [Managed Gateway](/docs/infrastructure/managed-gateway.md) for what each one writes, how to call it, local development and limits.

## Examples

```bash
# Object storage and a cache
forklaunch infra add billing object-store
forklaunch infra add billing cache

# Managed gateway capabilities
forklaunch infra add notifications email
forklaunch infra add notifications sms
forklaunch infra add clinic whatsapp
forklaunch infra add clinic voice
forklaunch infra add billing payments
forklaunch infra add coach models

# A worker can hold any managed gateway capability
forklaunch infra add reminders-worker sms

# Preview, then undo
forklaunch infra add coach models --dryrun
forklaunch infra remove coach models

# Afterwards
forklaunch score --offline
```

## What It Refuses

- A capability the project already has (on `add`) or does not have (on `remove`).
- `object-store` or `cache` on a worker, and any capability on a library.
- A project name that is not in the manifest.

## Workers and Events

A worker can send, call, charge or call a model. Events (delivery receipts, replies, call and payment events) are delivered over HTTP to `/platform-events/<capability>`, never to the worker process. When you add a capability that has events to a worker and no HTTP service holds it, the CLI prints a note:

```
note: 'reminders-worker' can call the sms gateway, but sms events are delivered over HTTP
to /platform-events/sms, never to the worker process. ...
also run `forklaunch infra add <service> sms` on an HTTP service so the events have somewhere to go.
```

If the app needs the events, add the same capability to an HTTP service too. `models` has no events, so it prints nothing.

## Related

- [Managed Gateway](/docs/infrastructure/managed-gateway.md) - using email, SMS, WhatsApp, voice, payments and models
- [Object Storage](/docs/infrastructure/storage.md)
- [Caches](/docs/infrastructure/caches.md)
