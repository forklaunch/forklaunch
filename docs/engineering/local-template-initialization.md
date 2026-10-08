# Authenticated local generation and release preparation

Every `forklaunch init` / `add` command requires the selected user's login before prompting or creating files. `release create --prepare-only` also requires user login; it only prepares files and never publishes them. It cannot substitute an HMAC environment variable for the user's login. Normal release publication retains its existing authenticated server checks.

These commands use the CLI's existing account selection, expiration and refresh handling. A cached token is not independently revalidated with the server on every local command. Server operations still enforce authorization.

Local execution and network isolation remain supported. The desktop must authorize work as its logged-in user while keeping credentials outside generated code and package scripts. Its previous credential-free preparation invocation must be replaced before shipping this CLI with the desktop. Do not mount the user's account directory into the generated-code worker.

Run `cli/tests/local_commands_require_auth.sh` against the newly built CLI to verify missing and expired logins reject initialization and release preparation before creating project or release files.
