---
name: gstack
description: "GStack (Garry Tan/YC): Claude Code workflow skills for planning, review, QA, shipping, and deployment. Use alongside ForkLaunch skills."
user-invokable: true
---

# GStack Integration

GStack is Garry Tan's (YC CEO) open-source Claude Code workflow framework. It provides structured skills for the full development lifecycle. Use these alongside ForkLaunch skills for maximum velocity.

Source: github.com/garrytan/gstack

## Key Workflows for ForkLaunch Projects

### Planning Phase

| Command | What It Does | When to Use |
|---------|-------------|-------------|
| `/office-hours` | YC-style product discovery. Six forcing questions before code. | Before starting a new feature or service |
| `/plan-ceo-review` | "What is the 10-star product hiding in this request?" Four scope modes: Expansion, Selective, Hold, Reduction. | Before scoping a major feature |
| `/plan-eng-review` | Engineering lead review. Forces diagrams (sequence, state, component, data-flow, test matrices). | Before implementing anything complex |

### Implementation Phase

| Command | What It Does | When to Use |
|---------|-------------|-------------|
| `/review` | Paranoid staff engineer code review. Catches N+1 queries, race conditions, stale reads, trust boundary violations. | Before every PR |
| `/investigate` | Systematic root-cause debugger. Stops after 3 failed fixes to question architecture. Auto-activates `/freeze`. | When debugging production issues |

### Quality Phase

| Command | What It Does | When to Use |
|---------|-------------|-------------|
| `/browse` | Persistent Chromium for visual testing. ~100-200ms per command. | Testing UI flows |
| `/qa` | Reads git diff, identifies affected pages, tests each systematically. | Before shipping any PR |
| `/cso` | OWASP Top 10 + STRIDE threat modeling. | Before launching compliance-sensitive features |

### Shipping Phase

| Command | What It Does | When to Use |
|---------|-------------|-------------|
| `/ship` | Syncs main, runs tests, audits coverage, pushes, creates PR. | When ready to ship |
| `/land-and-deploy` | Merge, deploy, verify in one command. | After PR approval |
| `/canary` | Post-deploy monitoring loop via browser. | After every production deploy |

### Design Phase

| Command | What It Does | When to Use |
|---------|-------------|-------------|
| `/design-consultation` | Build complete design system from scratch. Generates `DESIGN.md`. | New project without design |
| `/design-html` | Convert mockups to production HTML. Detects React/Vue/Svelte. | Implementing designs |
| `/design-review` | 80-item visual audit with auto-fix loop. One commit per fix. | Before shipping UI changes |

## Recommended Workflow with ForkLaunch

### New Feature
```
/office-hours          → Scope the feature
/plan-eng-review       → Lock architecture
[ForkLaunch skills]    → Implement using backend-patterns, common-tasks
/review                → Code review
/qa                    → Test
/ship                  → Create PR
```

### New Service
```
/plan-ceo-review       → Challenge scope
/plan-eng-review       → Design architecture
/studio                → Scaffold with ForkLaunch CLI
[implement]            → Build domain logic
/cso                   → Security audit (especially for PHI/PCI)
/review + /qa          → Review and test
/ship                  → Ship
```

### Debugging Production
```
/investigate           → Root-cause analysis (auto-freezes edits)
[fix]                  → Apply fix
/qa                    → Verify fix
/ship + /land-and-deploy → Ship and deploy
/canary                → Monitor post-deploy
```

## Safety Commands

| Command | What It Does |
|---------|-------------|
| `/careful` | Warns before destructive commands (rm -rf, DROP TABLE, git push --force) |
| `/freeze <dir>` | Restrict all edits to a single directory |
| `/guard` | `/careful` + `/freeze` combined. Use for production work. |
| `/unfreeze` | Remove freeze boundary |

## Integration Notes

- GStack's `/plan-ceo-review` and `/plan-eng-review` overlap with ForkLaunch's own plan skills. Use GStack's versions for the broader workflow integration. Use ForkLaunch's versions for framework-specific architecture decisions.
- GStack's `/review` catches general code issues. ForkLaunch's compliance skill catches data-classification and HIPAA-specific issues. Run both.
- GStack's `/browse` and `/qa` are essential for frontend testing of ForkLaunch-generated client portals and dashboards.
