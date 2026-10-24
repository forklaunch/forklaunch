# `forklaunch init agent` in a TypeScript app: the agent is a service tagged
# variant = "agent" with the agent routes, tools and agent.config.ts, and the
# command refuses bad input. The Rust agent is covered by
# rust_generated_projects.sh.
#
# Installing and building needs a @forklaunch/core that exports ./agent. Until
# one is published, set FORKLAUNCH_AGENT_CORE_TARBALL to a `pnpm pack` of
# framework/core to build and test the app as well.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
set -euo pipefail

fail() { echo "init_agent: $*" >&2; exit 1; }
has() { grep -qF -- "$2" "$1" || fail "$1 is missing: $2"; }

rm -rf output/init-agent
mkdir -p output/init-agent
cd output/init-agent

# No IAM: refused, nothing written.
RUST_BACKTRACE=1 $FL init application agent-no-iam -p agent-no-iam -o src/modules -d postgresql -f prettier -l oxlint -v zod -F express -r node -t vitest -D "Agent test" -A "Rohin Bhargava" -L 'MIT'
if $FL init agent assistant -p agent-no-iam/src/modules 2>/dev/null; then fail "init agent ran without IAM"; fi
[ ! -d agent-no-iam/src/modules/assistant ] || fail "init agent without IAM wrote files"

RUST_BACKTRACE=1 $FL init application agent-app -p agent-app -o src/modules -d postgresql -f prettier -l oxlint -v zod -F express -r node -t vitest -m iam-better-auth -D "Agent test" -A "Rohin Bhargava" -L 'MIT'
RUST_BACKTRACE=1 $FL init service records -d postgresql -p agent-app/src/modules -D "Records"

for bad in "--service nope" "--mcp linear" "--mcp Linear=https://mcp.linear.app/mcp" "--mcp linear=ftp://x"; do
    # shellcheck disable=SC2086
    if $FL init agent assistant -p agent-app/src/modules $bad 2>/dev/null; then fail "init agent accepted $bad"; fi
done
[ ! -d agent-app/src/modules/assistant ] || fail "a refused init agent wrote files"

RUST_BACKTRACE=1 $FL init agent assistant -p agent-app/src/modules --dryrun
[ ! -d agent-app/src/modules/assistant ] || fail "--dryrun wrote files"

RUST_BACKTRACE=1 $FL init agent assistant -p agent-app/src/modules -D "Assistant" \
    --instructions "You help staff with records." --service records \
    --mcp my_linear=https://mcp.linear.app/mcp

agent=agent-app/src/modules/assistant
for file in agent.config.ts tools/currentTime.tool.ts tools/index.ts \
    api/controllers/agent.controller.ts api/routes/agent.routes.ts __test__/agent.test.ts; do
    [ -f "$agent/$file" ] || fail "missing $agent/$file"
done
manifest=agent-app/.forklaunch/manifest.toml
grep -A3 'name = "assistant"' "$manifest" | grep -qF 'variant = "agent"' || fail "the agent is not tagged variant = \"agent\""
has "$agent/agent.config.ts" "from '@forklaunch/core/agent'"
has "$agent/agent.config.ts" 'serviceTools("records")'
has "$agent/agent.config.ts" 'process.env.MY_LINEAR_MCP_URL'
has "$agent/agent.config.ts" '"You help staff with records."'
has "$agent/server.ts" "import { agentRouter } from './api/routes/agent.routes';"
has "$agent/server.ts" 'app.use(agentRouter);'
has "$agent/.env.local" 'RECORDS_URL=http://localhost:'
has "$agent/.env.local" 'MY_LINEAR_MCP_URL=https://mcp.linear.app/mcp'
grep -qE '^HMAC_SECRET_KEY=.+' "$agent/.env.local" || fail "no HMAC_SECRET_KEY in .env.local"
has agent-app/docker-compose.yaml 'RECORDS_URL: http://records:'
has agent-app/docker-compose.yaml 'MY_LINEAR_MCP_URL: https://mcp.linear.app/mcp'

# A second agent with the same name is refused.
if $FL init agent assistant -p agent-app/src/modules 2>/dev/null; then fail "init agent reused a project name"; fi

if [ -n "${FORKLAUNCH_AGENT_CORE_TARBALL:-}" ]; then
    cd agent-app/src/modules
    sed -i.bak "s#^  '@forklaunch/core': .*#  '@forklaunch/core': file:${FORKLAUNCH_AGENT_CORE_TARBALL}#" pnpm-workspace.yaml
    rm pnpm-workspace.yaml.bak
    pnpm install
    pnpm build
    (cd assistant && pnpm vitest run __test__/agent.test.ts)
fi

echo "init_agent: ok"
