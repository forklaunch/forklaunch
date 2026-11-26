---
title: Domain Command
category: CLI Reference
description: Attach a custom domain to a platform application, route subdomains to services, and send email from the domain.
---

## Overview

`forklaunch app domain` manages the custom domain of the platform application your checkout is linked to (see [integrate](/docs/cli/integrate.md)). Every subcommand reads the application id from `.forklaunch/manifest.toml`, so run it from the application root or pass `--path`.

## Usage

```bash
forklaunch app domain <subcommand> [options]
```

| Subcommand | What it does |
| :--------- | :----------- |
| `status` | Show the custom domain and its validation records |
| `set <domain> [--mode delegated\|manual]` | Attach a custom domain |
| `remove` | Detach the custom domain |
| `subdomain add <prefix> -s <service-id> [-e <env>]` | Route a subdomain to one service |
| `subdomain remove <id>` / `subdomain list` | Remove or list subdomain routes |
| `email set\|status\|remove` | Send the app's managed-gateway email from the custom domain (Pro) |

All subcommands accept `-p, --path <path>` (application root).

## Custom domain

```bash
forklaunch app domain set app.example.com      # delegated (default)
forklaunch app domain set example.com --mode manual
forklaunch app domain status                   # validation status and DNS records
forklaunch app domain remove
```

Delegated mode asks you to point the domain's nameservers at the platform, which then manages its records. Use `--mode manual` for an apex domain whose MX records you need to keep. Either way the domain is not live until its validation records resolve.

## Send email from your domain (Pro)

By default an app's managed-gateway email (`forklaunch infra add <service> email`) goes from `no-reply@<app label>.<platform sending domain>`. On the Pro and Enterprise plans each environment can send from the application's custom domain instead.

```bash
forklaunch app domain email set -e production --from billing --name "Acme Billing"
forklaunch app domain email set -e staging --from billing --subdomain mail   # billing@mail.acme.com
forklaunch app domain email status                 # every environment
forklaunch app domain email status -e production --json
forklaunch app domain email remove -e staging      # asks first; -y to skip
```

### `email set`

| Option | Description |
| :----- | :---------- |
| `-e, --environment <env>` | Required. The environment whose email uses the custom domain |
| `--from <local-part>` | The part before the `@`. Lowercase letters, digits and `. _ + -`, not starting or ending with a dot, at most 64 characters. Default `no-reply` |
| `--name <display name>` | Shown to recipients. One line, at most 64 characters, no `<`, `>` or `"` |
| `--subdomain <sub>` | Send from a subdomain of the custom domain: one to three DNS labels (`mail` sends from `mail.<custom domain>`) |

It prints the from address (`Acme Billing <billing@acme.com>`), the verification status and, unless the platform wrote them for you, the DNS records to publish. If the environment has not been deployed with email yet, the sender is saved as `pending_verification` with a message saying so and no DNS records; its next deploy sets it up.

### `email status`

`-e, --environment <env>` shows one environment; without it every environment that has a sender is listed. `--json` prints the platform's response unchanged: always an array, `[]` when nothing is set or the application has no custom domain. Reading is not plan-gated; a sender whose plan lapsed shows `status: failed` with the plan message.

### `email remove`

`-e, --environment <env>` is required. Asks for confirmation unless `-y, --yes` is passed, and refuses without a terminal so a script cannot hang on the prompt. The environment goes back to the default platform sender. If no sender is set (the platform answers 404) it says so and exits successfully.

### Verification and DNS

- **Nothing breaks while you wait.** Until Amazon SES verifies the domain (`status: pending_verification`), whenever it refuses it, and while the plan lacks custom domains, mail keeps going from the default `no-reply@<app label>.<platform sending domain>` address. Sends never fail because of a sender that is still verifying or that `failed`.
- **Gave up**: SES stops checking after 72 hours (`status: failed`). Fix the records and run `set` again.
- **Delegated domain**: the three DKIM CNAMEs are written for you (`dnsRecordsWritten: true`); there is nothing required to publish.
- **Manual domain**: publish the printed CNAME records at your DNS provider, then run `forklaunch app domain email status` until it says `verified`.
- A `_dmarc` TXT record is included as **advisory**. Verification does not wait on it, but mailbox providers deliver better when you publish one.

### Errors

| Status | Meaning |
| :----- | :------ |
| 403 | `Custom email senders require a Pro or Enterprise plan`. Upgrade the organization's plan |
| 400 | Validation, for example the application has no custom domain yet (`forklaunch app domain set` first) |
| 404 | `remove` only: no sender is set for that environment |
| 502 / 503 | The mail provider is unavailable; try again |

## Managed instances

A managed instance has no application custom domain, so it takes the domain explicitly. See [Managed Gateway](/docs/infrastructure/managed-gateway.md#send-from-your-own-domain-pro).

```bash
forklaunch managed instance email set --id <instance-id> --domain clinic.com --from care --name "Clinic Care"
forklaunch managed instance email status --id <instance-id> [--json]
forklaunch managed instance email remove --id <instance-id> [-y]
```

Editors set and remove, viewers read. `status` adds `in use` (`active`): whether mail goes out from the sender right now. 400 = invalid domain, not under a public suffix, or the platform's own sending domain; 409 = another organization sends from or holds the domain; 502/503 = the mail provider or the ownership check is unavailable.

## Related

- [Managed Gateway](/docs/infrastructure/managed-gateway.md)
- [Infra command](/docs/cli/infra.md)
- [Deploy command](/docs/cli/deploy.md)
