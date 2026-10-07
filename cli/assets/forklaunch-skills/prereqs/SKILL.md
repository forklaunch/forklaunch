---
name: prereqs
description: "Bring a bare machine up to a state where ForkLaunch can build and deploy: Node, a package manager, git, a container runtime, the CLI, and login. Agent-executed, with a confirm before anything is installed. Use when a tool is missing, or before a first build."
user-invokable: true
---

# Prerequisites — getting a bare machine ready

## When to Use This Skill

- The user has never built anything on this machine, and nothing is installed
- Any ForkLaunch command failed because a tool was missing
- Before the first `forklaunch init` of a session, as a pre-flight
- `SETUP.md` in this pack is the same ground written for a **human to read**.
  This skill is for **you to execute**. Use SETUP.md when the user wants to do
  it themselves; use this skill when they want you to do it for them.

## What is actually required, and when

Two of these are needed earlier than people expect. `forklaunch` itself is a
single binary that needs nothing — but two commands in the normal flow shell
out to the Node toolchain:

| tool | needed for | needed by |
|---|---|---|
| **Node 22+** | everything below it | `forklaunch init`, `release create` |
| **pnpm** (or bun) | installing the app's dependencies | `forklaunch init`, `release create` |
| **git** | version control, and the GitHub App path | `release create --git`, autodeploy |
| **container runtime** | running the app **locally** | `docker compose up`, local dev only |
| **forklaunch CLI** | all of it | everything |
| **a ForkLaunch account** | anything that touches the platform | `app create`, `release`, `deploy` |

The container runtime is the one that is **not** required to ship. The platform
builds your images. If the user only wants to get something deployed and does
not need to run it on their own machine, you can skip it and say so — it is the
slowest install of the five.

## Rule: ask before you install

Installing software changes the user's machine in ways they did not explicitly
ask for and cannot trivially undo. **Run the detection block first, tell them
what is missing and what you would install, and get a yes.** One yes covers the
whole list; do not ask five times.

Say it like this:

> "Your machine is missing Node (the engine that runs your app) and pnpm (the
> tool that downloads the pieces it depends on). I can install both — about two
> minutes, and it only adds programs, it doesn't change anything you already
> have. Want me to?"

If they would rather do it themselves, send them to `SETUP.md` and stop.

## 1. Detect

Run all of it in one go. Every line is safe and read-only.

```bash
echo "os:      $(uname -s)"
echo "arch:    $(uname -m)"
echo "node:    $(node --version 2>/dev/null || echo MISSING)"
echo "pnpm:    $(pnpm --version 2>/dev/null || echo MISSING)"
echo "bun:     $(bun --version 2>/dev/null || echo MISSING)"
echo "git:     $(git --version 2>/dev/null || echo MISSING)"
echo "gitmail: $(git config user.email 2>/dev/null || echo UNSET)"
echo "docker:  $(docker --version 2>/dev/null || echo MISSING)"
echo "dockerd: $(docker ps >/dev/null 2>&1 && echo RUNNING || echo 'NOT RUNNING')"
echo "fl:      $(forklaunch --version 2>/dev/null || echo MISSING)"
echo "brew:    $(brew --version 2>/dev/null | head -1 || echo MISSING)"
```

Two results are easy to misread:

- **`docker` present but `dockerd` NOT RUNNING** is the common case, not a
  broken install. The runtime is installed and the app is closed. Ask the user
  to open OrbStack (or Docker Desktop); you cannot start it for them reliably.
- **A Node version below 22** is a *replace*, not an *install*. Say so, because
  it may affect their other projects.

## 2. Install what is missing

Install in the order below — each one depends on the ones above it.

### macOS

Homebrew first, because everything else rides on it. Homebrew's own installer
is interactive and asks for a password (it needs `sudo` once); if you are not
on a real terminal, hand the user this line to run themselves rather than
trying to drive it:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

> Tell the user: type `! <command>` in the Claude Code prompt to run something
> themselves and have the output land back in this conversation.

Then:

```bash
brew install node          # Node 22+ and npm
corepack enable pnpm       # pnpm, shipped with Node — no separate download
brew install git
```

**Container runtime — install OrbStack, not Docker Desktop.**

```bash
brew install --cask orbstack
```

OrbStack is the right default on macOS: it starts in seconds rather than
minutes, idles at a fraction of the memory, and is free for personal use. It
provides the same `docker` and `docker compose` commands, so nothing downstream
changes. Docker Desktop also works if the user already has it or their employer
requires it — do not make them switch.

**OrbStack must be opened once after installing.** It is not a background
service until it has run:

```bash
open -a OrbStack
```

Then re-check with `docker ps`. If it still fails, the app has not finished
starting — wait and try once more before treating it as broken.

### Linux

```bash
# Node 22 via nodesource
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt-get install -y nodejs git
corepack enable pnpm

# Docker Engine
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
```

The `usermod` line **only takes effect in a new login session.** Until the user
logs out and back in, `docker ps` fails with a permission error that looks like
a broken install. Say that explicitly rather than letting them discover it.

### Windows

Native Windows is not the supported path. Install WSL2 and work inside it:

```powershell
wsl --install -d Ubuntu
```

Then follow the Linux steps **inside the Ubuntu shell**, and keep the project
files on the Linux side (`~/…`, not `/mnt/c/…`) — cross-filesystem installs are
slow enough to look like a hang.

## 3. The ForkLaunch CLI

The CLI is a standalone binary. It does not need Node, and it keeps itself
updated after the first install.

```bash
curl -fsSL https://forklaunch.com/install.sh | bash
```

It installs to `~/.forklaunch/bin`. If `forklaunch --version` still says
`MISSING` afterwards, that directory is not on `PATH` — **open a new terminal
before concluding anything is wrong.** That single step resolves most
"the install didn't work" reports.

## 4. Identity

Two logins, and they are unrelated to each other.

```bash
git config --global user.name  "Their Name"
git config --global user.email "their@email"
```

```bash
forklaunch login       # opens a browser
forklaunch whoami      # confirms which account, org, role and plan
```

`whoami` also prints the **plan**. Read it now, because it decides what will be
possible later: a free plan caps components, monthly deploys and instance size,
and the deploy is refused at those limits rather than degraded. If the user is
on free and plans to deploy more than a toy, say so before they build, not
after.

For CI or a headless machine, `forklaunch login --token <API_TOKEN>` (or
`FORKLAUNCH_API_TOKEN`) replaces the browser flow.

## 5. Verify, then say what happened

Re-run the detection block. Every line should report a version.

Then report in plain language — what they now have, in one line each, and
anything still outstanding:

> "Your machine is ready. You now have Node (runs your app's code), pnpm
> (fetches the pieces it depends on), git (tracks changes), and OrbStack (runs
> a database on your machine for testing). You're signed in as
> alex@example.com on the Free plan — that allows up to 3 components, which is
> fine for what we planned."

## Failure modes worth naming

**`command not found` right after a successful install.** The shell that is
running was started before the program existed. Open a new terminal. This is
the single most common false alarm.

**`docker ps` fails but `docker --version` works.** The runtime is installed and
not running. On macOS, `open -a OrbStack`. On Linux, `sudo systemctl start
docker`, and check the `usermod` group change has taken effect in a new login.

**`pnpm install` fails on a fresh scaffold with a lockfile error.** ForkLaunch
pins a minimum release age for dependencies; a package published in the last 24
hours can fail the gate. Retry, or use `--no-frozen-lockfile` once.

**Node is installed but too old.** `corepack enable pnpm` needs Node 16+, and
ForkLaunch needs 22+. Upgrading Node is a machine-wide change — tell the user
before doing it, since it affects their other projects.

**Homebrew asks for a password and nothing happens.** `sudo` prompts cannot be
answered from a non-interactive session. Hand the command to the user with the
`!` prefix instead of retrying.

## Plain-English summary

A brand-new machine needs five things: **Node** (runs the code), **pnpm**
(downloads the pieces the code depends on), **git** (tracks changes),
**OrbStack** (runs a database on the machine, only needed to test locally), and
the **ForkLaunch CLI** itself — plus signing in.

Only the first two and the CLI are needed to *deploy*; the container runtime is
for running the app on their own computer, so it can be skipped if they just
want something live. Check what is missing first, tell the user what you would
install and get a yes before installing anything, and when a command still
isn't found after installing it, open a new terminal before assuming it failed.

## Related

- `SETUP.md` — the same ground, written for the user to follow by hand
- `/getting-started` — the driver skill; comes straight back here on a failed check
- `/cli` — every command, once the machine is ready
