---
title: Dice Roll Node App
category: Examples
description: Build a dice rolling API with Node.js, Express, and PostgreSQL using ForkLaunch, from CLI scaffold to running endpoints in a few hours.
---

## Overview

This tutorial walks through building a dice roll API with ForkLaunch. The app accepts a number of sides, rolls the die, persists the result to PostgreSQL, and returns roll statistics. The full source code is on GitHub.

<a href="https://github.com/srtandon/dicey-roll" target="_blank" rel="noopener noreferrer" style="display:inline-flex;align-items:center;gap:8px;padding:8px 16px;border-radius:8px;border:1px solid #E34C26;color:#E34C26;font-weight:600;font-size:14px;text-decoration:none;margin-bottom:16px;">
  <svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0 1 12 6.844a9.59 9.59 0 0 1 2.504.337c1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.02 10.02 0 0 0 22 12.017C22 6.484 17.522 2 12 2z"/></svg>
  View on GitHub: srtandon/dicey-roll
</a>

**What you will build:**
- `POST /dice-rtr/roll`: rolls a die with the specified number of sides and persists the result
- `GET /dice-rtr/stats`: returns total rolls and distribution grouped by die type

**Prerequisites:**
- ForkLaunch CLI installed (`npm install -g forklaunch`)
- Node.js 18+ and pnpm 8+
- Docker (for PostgreSQL)

---

## Quick Launch

Five commands to get the scaffold running:

```bash
# 1. Create the application
forklaunch init app dice-roll-node-app --database postgresql --runtime node
```

```bash
# 2. Add the service
cd dice-roll-node-app
forklaunch init service roll-dice-svc --database postgresql

# 3. Add a router to the service
forklaunch init router dice-rtr --path ./src/modules/roll-dice-svc

# 4. Install and migrate
pnpm install && pnpm migrate:init && pnpm migrate:create && pnpm migrate:up

# 5. Start the dev server
pnpm dev
```

The server starts on `http://localhost:8000`. ForkLaunch generates OpenAPI docs at `http://localhost:8000/docs`.

---

## Try It Here

<DiceRollDemo />

---

## Full Walkthrough

### Step 1: Create the Application

```bash
forklaunch init app dice-roll-node-app \
  --path ./dice-roll-node-app
  --database postgresql \
  --validator zod \
  --http-framework express \
  --runtime node
```

**What gets generated:**
- Application structure under `src/`
- `.forklaunch/manifest.toml`: source of truth for project configuration
- `docker-compose.yaml` for local development (includes Grafana, Prometheus, Loki, Tempo)
- Base configuration and TypeScript config files

### Step 2: Add a Service

```bash
cd dice-roll-node-app
forklaunch init service roll-dice-svc --path dice-roll-node-app/src/modules --database postgresql
```

This generates a complete service with routes, controllers, services, entities, and mappers, wired into docker-compose automatically.

### Step 3: Add a Router

```bash
forklaunch init router dice-rtr --path ./src/modules/roll-dice-svc
```

This adds a new [RCSIDES](/docs/learn/artifacts.md#rcsides-architecture-pattern) route/controller stack to the service and wires it into `server.ts`.

### Step 4: Configure Environment Variables

Create `.env.local` in the application root:

```bash
DB_NAME=dice-roll-node-app-dev
DB_HOST=localhost
DB_USER=postgresql
DB_PASSWORD=postgresql
DB_PORT=5432

PORT=8000
NODE_ENV=development
HOST=0.0.0.0
PROTOCOL=http
VERSION=v1
DOCS_PATH=/docs

OTEL_SERVICE_NAME=dice-roll-node-app
OTEL_LEVEL=info
```

### Step 5: Update the Entity

Edit `src/modules/roll-dice-svc/persistence/entities/diceRtrRecord.entity.ts`:

```typescript
import { Entity, Property } from '@mikro-orm/core';
import { SqlBaseEntity } from '@dice-roll-node-app/core';

@Entity()
export class DiceRtrRecord extends SqlBaseEntity {
  @Property()
  sides!: number;

  @Property()
  result!: number;
}
```

### Step 6: Update Schemas

Edit `src/modules/roll-dice-svc/domain/schemas/diceRtr.schema.ts`:

```typescript
import { number, string } from '@dice-roll-node-app/core';

export const DiceRtrRollRequestSchema = {
  sides: number
};

export const DiceRtrRollResponseSchema = {
  sides: number,
  result: number,
  id: string,
  createdAt: string
};

export const DiceRtrStatsResponseSchema = {
  totalRolls: number,
  distribution: {
    sides: number,
    count: number,
    average: number,
    min: number,
    max: number
  }
};
```

### Step 7: Update the Service

Edit `src/modules/roll-dice-svc/services/diceRtr.service.ts`:

```typescript
import { DiceRtrRecord } from '../persistence/entities/diceRtrRecord.entity';

export class BaseDiceRtrService implements DiceRtrService {
  diceRtrRoll = async (dto: DiceRtrRollRequestDto): Promise<DiceRtrRollResponseDto> => {
    if (dto.sides < 2) {
      throw new Error(`sides must be at least 2, got ${dto.sides}`);
    }

    const result = Math.floor(Math.random() * dto.sides) + 1;

    const entity = this.entityManager.create(DiceRtrRecord, {
      sides: dto.sides,
      result
    });

    await this.entityManager.persistAndFlush(entity);

    this.openTelemetryCollector.info('Dice rolled', { sides: dto.sides, result });

    return { sides: dto.sides, result, id: entity.id, createdAt: entity.createdAt.toISOString() };
  };

  diceRtrStats = async (): Promise<DiceRtrStatsResponseDto> => {
    const allRolls = await this.entityManager.find(DiceRtrRecord, {});

    const grouped: Record<number, number[]> = {};
    for (const roll of allRolls) {
      (grouped[roll.sides] ??= []).push(roll.result);
    }

    const distribution = Object.entries(grouped).map(([sides, results]) => ({
      sides: Number(sides),
      count: results.length,
      average: results.reduce((s, n) => s + n, 0) / results.length,
      min: Math.min(...results),
      max: Math.max(...results)
    }));

    return { totalRolls: allRolls.length, distribution };
  };
}
```

### Step 8: Update the Controller

Edit `src/modules/roll-dice-svc/api/controllers/diceRtr.controller.ts`:

```typescript
import { handlers, schemaValidator } from '@dice-roll-node-app/core';
import {
  DiceRtrRollRequestSchema,
  DiceRtrRollResponseSchema,
  DiceRtrStatsResponseSchema
} from '../../domain/schemas/diceRtr.schema';
import { ci, tokens } from '../../bootstrapper';

const scopeFactory = () => ci.createScope();
const serviceFactory = ci.scopedResolver(tokens.DiceRtrService);

export const diceRtrRoll = handlers.post(
  schemaValidator,
  '/roll',
  {
    name: 'Roll Dice',
    summary: 'Roll a die with the specified number of sides',
    body: DiceRtrRollRequestSchema,
    responses: { 200: DiceRtrRollResponseSchema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory(scopeFactory()).diceRtrRoll(req.body));
  }
);

export const diceRtrStats = handlers.get(
  schemaValidator,
  '/stats',
  {
    name: 'Get Statistics',
    summary: 'Return roll statistics grouped by die type',
    responses: { 200: DiceRtrStatsResponseSchema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory(scopeFactory()).diceRtrStats());
  }
);
```

### Step 9: Update Routes

Edit `src/modules/roll-dice-svc/api/routes/diceRtr.routes.ts`:

```typescript
import { forklaunchRouter, schemaValidator } from '@dice-roll-node-app/core';
import { diceRtrRoll, diceRtrStats } from '../controllers/diceRtr.controller';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

export const diceRtrRouter = forklaunchRouter('/dice-rtr', schemaValidator, openTelemetryCollector);

diceRtrRouter.post('/roll', diceRtrRoll);
diceRtrRouter.get('/stats', diceRtrStats);
```

### Step 10: Run Migrations and Start

```bash
pnpm install
pnpm migrate:init
pnpm migrate:create
pnpm migrate:up
pnpm dev
```

You should see:

```
INFO: 🎉 RollDiceSvc Server is running at http://0.0.0.0:8000 🎉
```

### Step 11: Test the API

**Roll a d20:**

```bash
curl -X POST http://localhost:8000/dice-rtr/roll \
  -H "Content-Type: application/json" \
  -d '{"sides": 20}'
```

**Response:**

```json
{
  "sides": 20,
  "result": 15,
  "id": "ab45bbcb-40f1-425e-9a42-495b4065bf1b",
  "createdAt": "2025-11-12T07:03:17.634Z"
}
```

**Get statistics:**

```bash
curl http://localhost:8000/dice-rtr/stats
```

**Response:**

```json
{
  "totalRolls": 4,
  "distribution": [
    { "sides": 20, "count": 2, "average": 10.5, "min": 6, "max": 15 },
    { "sides": 6,  "count": 1, "average": 4,    "min": 4, "max": 4  },
    { "sides": 12, "count": 1, "average": 11,   "min": 11,"max": 11 }
  ]
}
```

### Step 12: View the Observability Dashboard

ForkLaunch includes a full observability stack out of the box. Open Grafana at `http://localhost:3000` to see request traces, logs, and service metrics; no additional setup required.

---

## Prompts I Used

This app was built using Claude Code, Cursor, and ChatGPT to scaffold and debug. Here are the prompts that got the most done.

**Prompt 1: Initial scaffold:**

> "Step-by-step guide to create a dice roll API using ForkLaunch based on the architecture in this repo: https://github.com/forklaunch/forklaunch-js. Use Node.js, Express, Zod validator, and PostgreSQL."

**Prompt 2: Generating the roll and stats endpoints:**

> "Based on this ForkLaunch service scaffold, generate the roll and stats endpoints with PostgreSQL persistence using MikroORM. The roll endpoint should accept a `sides` parameter, compute a random result, and save to the database. The stats endpoint should return total rolls and a distribution grouped by die type."

**Prompt 3: Debugging migrations:**

> "I'm getting a MikroORM migration error when running `pnpm migrate:up`. The entity has `sides` and `result` as required properties. Here is the error: [paste error]. How do I fix this?"

After a few hours of iteration and debugging, the API was running end-to-end with OpenTelemetry observability included automatically by ForkLaunch.

---

## Troubleshooting

**Port already in use:** Change `PORT` in `.env.local`.

**Database connection errors:** Verify PostgreSQL is running:

```bash
docker-compose ps
docker-compose logs postgres
```

**Migration errors:** Check migration status:

```bash
pnpm migrate:status
```

**Service not starting:** Look for missing environment variables in the startup log. ForkLaunch validates required env vars at boot and prints which ones are missing.

---

## Next Steps

- [Creating an Application](/docs/creating-an-application.md): Full walkthrough of building a multi-service system
- [Local Development](/docs/local-development.md): docker-compose, hot reload, and database setup
- [Framework Reference: Telemetry](/docs/framework/telemetry.md): OpenTelemetry configuration
- [Adding Projects](/docs/adding-projects.md): Add workers, libraries, and additional services
