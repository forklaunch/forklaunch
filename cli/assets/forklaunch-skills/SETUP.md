# Setting up your machine

This guide takes you from a brand-new computer to building an app with
ForkLaunch. No prior experience assumed. If you have never opened a terminal
before, that is fine — this tells you exactly what to type.

**Roughly 30 minutes**, most of it waiting for downloads.

---

## First: opening a terminal

The terminal is a window where you type commands instead of clicking.

- **Mac** — press `Cmd + Space`, type `Terminal`, press Enter.
- **Windows** — press the Start button, type `PowerShell`, press Enter.
- **Linux** — press `Ctrl + Alt + T`.

Throughout this guide, "run this" means: click the terminal window, type the
line, and press Enter.

**How to tell if something worked.** After each step there is a *check*. Run
it. If it prints a version number, that step worked. If it says something like
`command not found`, that step did not work — close the terminal, open a new
one, and try the check again before redoing the install. Programs often only
become available in a *new* terminal window.

---

## Step 1 — Node

**What it is:** the engine that runs your app's code. Nothing else works
without it.

**Install:** go to [nodejs.org](https://nodejs.org) and download the version
labelled **LTS** (it stands for "long-term support" — the stable one). Open the
downloaded file and click through the installer, accepting the defaults.

**Check** — open a *new* terminal and run:

```bash
node --version
```

You want **v22** or higher. If you see a lower number, install the LTS version
from the site above; that will replace the old one.

---

## Step 2 — A container runtime

**What it is:** your app needs a database to store things. Rather than
installing a database directly, this runs one in a self-contained box on your
machine that you can start and throw away. That box is called a container.

**Mac — install OrbStack** (lighter and faster than the alternative):
download from [orbstack.dev](https://orbstack.dev), open the downloaded file,
and drag OrbStack into Applications. **Open it once** — it needs to be running,
not just installed.

**Windows or Linux — install Docker Desktop** from
[docker.com/products/docker-desktop](https://www.docker.com/products/docker-desktop).
Same idea: install it, then open it.

**Check:**

```bash
docker ps
```

A small table with column headings (probably empty underneath) means it works.
`Cannot connect to the Docker daemon` means the app is installed but **not
running** — open OrbStack or Docker Desktop and wait for it to finish starting,
then run the check again.

> **This one catches people out.** OrbStack and Docker Desktop have to be
> *open*, like any other app. If your app suddenly stops working tomorrow, this
> is the first thing to check.

---

## Step 3 — Git and a GitHub account

**What it is:** Git saves snapshots of your work so you can go back. GitHub is
the website that stores those snapshots online.

**Install Git:**

- **Mac** — run `git --version`. If it isn't installed, macOS offers to install
  it for you. Click Install.
- **Windows** — download from [git-scm.com](https://git-scm.com) and accept the
  installer defaults.
- **Linux** — `sudo apt install git` (Ubuntu/Debian). It will ask for your
  computer password; typing shows nothing on screen, which is normal.

**Create a GitHub account:** go to [github.com](https://github.com) and sign
up. Free is fine. You will be asked to confirm your email address — do that
now, because some things won't work until you have.

**Tell Git who you are.** Run these two lines with your own name and the email
you used for GitHub:

```bash
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

This just labels your saved snapshots. Nothing is sent anywhere.

**Check:**

```bash
git --version
git config user.email
```

The second should print your email back.

---

## Step 4 — The ForkLaunch CLI

**What it is:** the tool that builds your app for you.

```bash
npm i -g forklaunch
```

`-g` means "make this available everywhere on my computer".

If that fails with a **permission** error on Mac or Linux, it is trying to
write somewhere it isn't allowed. Run it again with `sudo` in front — that
means "do this as the computer's administrator", and it will ask for your
password:

```bash
sudo npm i -g forklaunch
```

**Check:**

```bash
forklaunch --version
```

---

## Step 5 — Your coding assistant

You need one of these. They are what you actually talk to.

**Claude Code:**

```bash
npm i -g @anthropic-ai/claude-code
```

Then run `claude` and follow the sign-in prompts in your browser. You will need
an Anthropic account — it will walk you through creating one if you don't have
one.

**Codex** — follow the install instructions at
[openai.com/codex](https://openai.com/codex) and sign in when prompted.

Either works. Pick one.

---

## Step 6 — Sign in to ForkLaunch

```bash
forklaunch login
```

This opens your browser. Sign in, or create an account if you don't have one.
Once the browser says you're signed in, you can close that tab and return to
the terminal.

**What you are agreeing to:** signing in links the tool on your computer to
your ForkLaunch account, so it can create and deploy apps on your behalf. You
can sign out any time with `forklaunch logout`.

**Check:**

```bash
forklaunch whoami
```

Prints the account you're signed in as.

---

## Step 7 — Load the skills

```bash
forklaunch context
```

This copies ForkLaunch's instructions into your project so your coding
assistant knows how to build with it correctly. Without this it will guess, and
guess wrong.

> Re-run this whenever the CLI updates. The tool updates itself automatically;
> these instructions do not.

---

## You're ready

Start your assistant in a new, empty folder:

```bash
mkdir my-app
cd my-app
claude          # or: codex
```

Then say what you want to build, in your own words:

> "Read the getting-started skill, then help me build an app for tracking
> equipment repairs for a small garage."

It will ask you questions before writing anything. Answer them in plain
language — you do not need to know any technical terms. **Answer the one about
sensitive information carefully**: whether the app stores things like medical
details, card numbers or home addresses changes how it is built, and it is
expensive to change later.

---

## When something goes wrong

| what you see | what it means | what to do |
|---|---|---|
| `command not found` | installed, but this terminal doesn't know yet | close the terminal, open a new one |
| `Cannot connect to the Docker daemon` | container app isn't running | open OrbStack / Docker Desktop |
| `EACCES` / permission denied | needs administrator rights | put `sudo` in front (Mac/Linux) |
| `port already in use` | something else is using that door number | close other dev tools, or restart |
| the assistant suggests a command that doesn't exist | your instructions are out of date | run `forklaunch context` |

**A good habit:** if you get stuck, paste the error into your assistant and ask
what it means. That is what it is for, and it can see more context than a
troubleshooting table.

---

## What you installed, in one line each

| | |
|---|---|
| **Node** | runs your app's code |
| **OrbStack / Docker** | runs your app's database in a disposable box |
| **Git + GitHub** | saves your work, and keeps a copy online |
| **ForkLaunch CLI** | builds and deploys the app |
| **Claude Code / Codex** | the assistant you talk to |

You will not need to touch most of these again — they sit in the background.
The two you will actually interact with are your assistant and the terminal.
