---
title: Authentication Commands
category: CLI
description: Learn how to authenticate with ForkLaunch services.
---

## Overview

THIS COMMAND IS CURRENTLY UNDER DEVELOPMENT AND IS NOT YET AVAILABLE.

Manage authentication with the ForkLaunch platform.

## Usage

### Login

Authenticate with your ForkLaunch account.

```bash
forklaunch login
```

**Process:**

1. Opens browser to ForkLaunch login page
2. Handles OAuth authentication flow
3. Stores credentials locally in `~/.forklaunch/`
4. Confirms successful authentication

```bash
$ forklaunch login
Opening browser for authentication...
Successfully authenticated as user@example.com
```

### Logout

Terminate session and remove stored credentials.

```bash
forklaunch logout
```

**Actions:**

- Clears local tokens
- Revokes server session (if connected)
- Confirms logout

```bash
$ forklaunch logout
Successfully logged out from ForkLaunch
```

### Whoami

Display current session information.

```bash
forklaunch whoami
```

**Shows:**

- Username/Email
- Organization (if applicable)
- Session status and expiry

```bash
$ forklaunch whoami
Logged in as: user@example.com
Organization: My Company
Session expires: 2024-12-31 23:59:59 UTC
```

When not authenticated:

```bash
$ forklaunch whoami
Not currently logged in to ForkLaunch
Run 'forklaunch login' to authenticate
```

## Workflow

```bash
# Check current session
forklaunch whoami

# Authenticate if needed
forklaunch login

# Use authenticated commands
forklaunch init application
forklaunch depcheck

# Logout when done (optional)
forklaunch logout
```

## Troubleshooting

**Error: Browser doesn't open automatically**:

- Copy the URL shown in the terminal and open it manually

**Error: "Authentication failed" error**:

1. Check your internet connection
2. Verify your credentials
3. Try logging out and logging in again

**Error: "Token expired" error**:

- Run `forklaunch login` to refresh your session

**Error: Commands say "not authenticated" after login**:

1. Run `forklaunch whoami` to check status
2. If still not authenticated, try `forklaunch logout` then `forklaunch login`

**Error: Need to switch accounts**:
**Solution**:

1. `forklaunch logout`
2. `forklaunch login` (authenticate with different account)

**Error: "Cannot write credentials" error**:

- Check that the ForkLaunch config directory is writable `~/.forklaunch/`

## Security Best Practices

**Shared Computers:**

- Always run `forklaunch logout` when finished
- Don't save credentials in browser if prompted
- Consider using incognito/private browsing mode

**Token Management:**

- Tokens are stored securely on your local machine
- Never share authentication tokens with others
- Regularly check `forklaunch whoami` to verify session status

**Enterprise Environments:**

- Follow your organization's authentication policies
- Use appropriate 2FA methods when required
- Contact your admin if experiencing SSO issues

## Related Commands

- [`forklaunch version`](./config#version) - Check CLI version and connectivity
- [`forklaunch init`](./init) - Initialize projects (requires authentication)
- [`forklaunch depcheck`](./depcheck) - Check dependencies (may require authentication)

## Related Documentation

- **[Getting Started](../getting-started)** - Initial setup and verification
- **[CLI Overview](../cli)** - Complete command reference
- **[Configuration](./config)** - Advanced configuration options
