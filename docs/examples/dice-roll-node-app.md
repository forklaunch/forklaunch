# Dice Roll Example - Complete "Hello World" Tutorial

This is a complete step-by-step guide to building a simple dice roll application with ForkLaunch. It demonstrates the full workflow from initialization to a working API with database persistence.

## What We'll Build

A dice roll API that:
- Accepts different die types (d4, d6, d12, d20, etc.)
- Rolls the dice and returns the result
- Saves each roll to the database
- Provides statistics on all rolls

## Prerequisites

- ForkLaunch CLI installed
- Node.js 18+ and pnpm 8+ (or Bun 1.22+)
- Docker (for PostgreSQL)
- Git

## Step 1: Create the Application

You can use interactive prompts or flags:

```bash
# Interactive mode (prompts for each option)
forklaunch init application dice-roll-node-app

# Or with flags (for scripts/AIs)
forklaunch init application dice-roll-node-app \
  --path ./dice-roll-node-app \
  -o src/modules \
  -d postgresql \
  -v zod \
  -f prettier \
  -l eslint \
  -F express \
  -r node \
  -t vitest \
  -D "Simple dice roll application" \
  -A "Your Name" \
  -L MIT
```

**What gets created:**
- Application structure with core, monitoring, and universal-sdk modules
- Docker Compose with monitoring services (Grafana, Prometheus, Loki, Tempo)
- Application artifacts (manifest, workspace config, etc.)

## Step 2: Add a Service

Add a service to host the dice roll API:

```bash
cd dice-roll-node-app

# Interactive mode
forklaunch init service roll-dice-svc

# Or with flags
forklaunch init service roll-dice-svc \
  --path ./src/modules \
  --database postgresql \
  --description "Dice roll API service"
```

**What gets created:**
- Service directory with complete [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) stack
- Routes and controllers (empty, ready to customize)
- Entity, schemas, types
- Service registered in all artifacts (manifest, docker-compose, workspace, SDK, tsconfig)

## Step 3: Add a Router

Add a router to the service for dice roll endpoints:

```bash
forklaunch init router dice-rtr \
  --path ./src/modules/roll-dice-svc \
  --description "Dice roll routes"
```

**What gets created:**
- New routes and controllers in the service
- Additional [RCSIDES](/docs/artifacts.md#rcsides-architecture-pattern) files for the router
- Router wired into `server.ts`

## Step 4: Configure Environment Variables

Create `.env.local` in the application root:

```bash
# Database Configuration
DB_NAME=dice-roll-node-app-dev
DB_HOST=localhost
DB_USER=postgresql
DB_PASSWORD=postgresql
DB_PORT=5432

# Server Configuration
PORT=8000
NODE_ENV=development
HOST=0.0.0.0
PROTOCOL=http
VERSION=v1
DOCS_PATH=/docs

# OpenTelemetry
OTEL_SERVICE_NAME=dice-roll-node-app
OTEL_LEVEL=info
```

## Step 5: Update Entity

Edit `src/modules/roll-dice-svc/persistence/entities/diceRtrRecord.entity.ts`:

```typescript
import { Entity, Property } from '@mikro-orm/core';
import { SqlBaseEntity } from '@dice-roll-node-app/core';

@Entity()
export class DiceRtrRecord extends SqlBaseEntity {
  @Property()
  dieType!: string; // e.g., "d4", "d6", "d12", "d20"

  @Property()
  result!: number; // The rolled value (1 to dieType number)
}
```

## Step 6: Update Schemas

Edit `src/modules/roll-dice-svc/domain/schemas/diceRtr.schema.ts`:

```typescript
import { number, string } from '@dice-roll-node-app/core';

export const DiceRtrRollRequestSchema = { 
  dieType: string
};

export const DiceRtrRollResponseSchema = {
  dieType: string,
  result: number
};

export const DiceRtrStatsResponseSchema = {
    totalRolls: number,
    byDieType: record(string, {
        count: number,
        average: number,
        min: number,
        max: number
    })
};
```

## Step 7: Update Mappers

Edit `src/modules/roll-dice-svc/domain/mappers/diceRtr.mappers.ts`:

```typescript
import {
  requestMapper,
  responseMapper
} from '@forklaunch/core/mappers';
import { schemaValidator } from '@dice-roll-node-app/core';
import { EntityManager } from '@mikro-orm/core';
import { DiceRtrRecord } from '../../persistence/entities/diceRtrRecord.entity';
import { DiceRtrRequestSchema, DiceRtrResponseSchema } from '../schemas/diceRtr.schema';

export const DiceRtrRequestMapper = requestMapper(
  schemaValidator,
  DiceRtrRequestSchema,
  DiceRtrRecord,
  {
    toEntity: async (dto, em: EntityManager) => {
      // Parse dieType (e.g., "d6" -> 6)
      const sides = parseInt(dto.dieType.replace('d', ''));
      if (isNaN(sides) || sides < 2) {
        throw new Error(`Invalid die type: ${dto.dieType}`);
      }
      
      // Roll the dice
      const result = Math.floor(Math.random() * sides) + 1;
      
      return DiceRtrRecord.create({
        dieType: dto.dieType,
        result: result,
        createdAt: new Date(),
        updatedAt: new Date(),
      }, em);
    }
  }
);

export const DiceRtrResponseMapper = responseMapper(
  schemaValidator,
  DiceRtrResponseSchema,
  DiceRtrRecord,
  {
    toDto: async (entity: DiceRtrRecord) => {
      return await entity.read();
    }
  }
);
```

## Step 8: Update Service

Edit `src/modules/roll-dice-svc/services/diceRtr.service.ts`:

```typescript
// ... existing imports ...
import { DiceRtrRecord } from '../persistence/entities/diceRtrRecord.entity';

export class BaseDiceRtrService implements DiceRtrService { 
  // ... existing constructor ...

  diceRtrRoll = async (
    dto: DiceRtrRollRequestDto
  ): Promise<DiceRtrRollResponseDto> => {
    // Validate die type
    const sides = parseInt(dto.dieType.replace('d', ''));
    if (isNaN(sides) || sides < 2) {
      throw new Error(`Invalid die type: ${dto.dieType}. Use format d4, d6, d12, d20, etc.`);
    }

    // Create entity with roll result
    const entity = await DiceRtrRollRequestMapper.toEntity(
      dto,
      this.entityManager
    );

    // Save to database
    await this.entityManager.persistAndFlush(entity);

    this.openTelemetryCollector.info('Dice rolled', {
      dieType: dto.dieType,
      result: entity.result
    });

    return DiceRtrRollResponseMapper.toDto(entity);
  };

  diceRtrStats = async (): Promise<DiceRtrStatsResponseDto> => {
    // Get all rolls
    const allRolls = await this.entityManager.find(DiceRtrRecord, {});

    // Group by die type and calculate stats
    const byDieType: Record<string, { count: number; average: number; min: number; max: number }> = {};

    allRolls.forEach(roll => {
      if (!byDieType[roll.dieType]) {
        byDieType[roll.dieType] = {
          count: 0,
          average: 0,
          min: Infinity,
          max: -Infinity
        };
      }

      const stats = byDieType[roll.dieType];
      stats.count++;
      stats.min = Math.min(stats.min, roll.result);
      stats.max = Math.max(stats.max, roll.result);
    });

    // Calculate averages
    Object.keys(byDieType).forEach(dieType => {
      const rolls = allRolls.filter(r => r.dieType === dieType);
      const sum = rolls.reduce((acc, r) => acc + r.result, 0);
      byDieType[dieType].average = sum / rolls.length;
    });

    return {
      totalRolls: allRolls.length,
      byDieType
    };
  };
}
```

## Step 9: Update Interface

Edit `src/modules/roll-dice-svc/domain/interfaces/diceRtr.interface.ts`:

```typescript
import {
  DiceRtrRequestDto,
  DiceRtrResponseDto,
  DiceRtrStatsResponseDto
} from '../types/diceRtr.types';

export interface DiceRtrService {
  diceRtrPost: (dto: DiceRtrRequestDto) => Promise<DiceRtrResponseDto>;
  diceRtrStats: () => Promise<DiceRtrStatsResponseDto>;
}
```

## Step 10: Update Controller

Edit `src/modules/roll-dice-svc/api/controllers/diceRtr.controller.ts`:

```typescript
import { handlers, schemaValidator } from '@dice-roll-node-app/core';
import { DiceRtrRequestMapper, DiceRtrResponseMapper } from '../../domain/mappers/diceRtr.mappers';
import { DiceRtrRequestSchema, DiceRtrResponseSchema, DiceRtrStatsResponseSchema } from '../../domain/schemas/diceRtr.schema';
import { ci, tokens } from '../../bootstrapper';

const scopeFactory = () => ci.createScope();
const serviceFactory = ci.scopedResolver(tokens.DiceRtrService);
const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

// POST endpoint handler that returns a simple message
export const diceRtrPost = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Dice Rtr Post',
    summary: 'Posts Dice Rtr',
    body: DiceRtrRequestSchema,
    responses: {
      200: DiceRtrResponseSchema
    }
  },
  async (req, res) => {
    res.status(200).json({
      message: 'hello, world!'
    });
  }
);

// Roll dice endpoint
export const diceRtrRoll = handlers.post(
  schemaValidator,
  '/roll',
  {
    name: 'Roll Dice',
    summary: 'Rolls a dice with specified number of sides',
    body: DiceRtrRollRequestMapper.schema,
    responses: {
      200: DiceRtrRollResponseMapper.schema
    }
  },
  async (req, res) => {
    res.status(200).json(
      await serviceFactory(scopeFactory()).diceRtrRoll(req.body)
    );
  }
);

// Stats endpoint
export const diceRtrStats = handlers.get(
  schemaValidator,
  '/stats',
  {
    name: 'Get Dice Roll Statistics',
    summary: 'Returns statistics about all dice rolls',
    responses: {
      200: DiceRtrStatsResponseSchema
    }
  },
  async (req, res) => {
    res.status(200).json(
      await serviceFactory(scopeFactory()).diceRtrStats()
    );
  }
);
```

## Step 11: Update Routes

Edit `src/modules/roll-dice-svc/api/routes/diceRtr.routes.ts`:

```typescript
import { forklaunchRouter, schemaValidator } from '@dice-roll-node-app/core';
import { diceRtrGet, diceRtrPost, diceRtrRoll, diceRtrStats } from '../controllers/diceRtr.controller';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

export const diceRtrRouter = forklaunchRouter(
  '/dice-rtr',
  schemaValidator, 
  openTelemetryCollector
);

// Mount the routes
export const diceRtrGetRoute = diceRtrRouter.get('/', diceRtrGet);
export const diceRtrPostRoute = diceRtrRouter.post('/', diceRtrPost);
export const diceRtrRollRoute = diceRtrRouter.post('/roll', diceRtrRoll);
export const diceRtrStatsRoute = diceRtrRouter.get('/stats', diceRtrStats);
```

## Step 12: Update Types

Edit `src/modules/roll-dice-svc/domain/types/diceRtr.types.ts` to add stats type:

```typescript
import { Schema } from '@forklaunch/validator';
import { SchemaValidator } from '@dice-roll-node-app/core';
import { DiceRtrRequestSchema, DiceRtrResponseSchema, DiceRtrRollRequestSchema, DiceRtrRollResponseSchema, DiceRtrStatsResponseSchema } from '../schemas/diceRtr.schema';


// Exported type that matches the request schema
export type DiceRtrRequestDto = Schema<typeof DiceRtrRequestSchema, SchemaValidator>;

// Exported type that matches the response schema
export type DiceRtrResponseDto = Schema<typeof DiceRtrResponseSchema, SchemaValidator>;

// Exported types for roll operations
export type DiceRtrRollRequestDto = Schema<typeof DiceRtrRollRequestSchema, SchemaValidator>;
export type DiceRtrRollResponseDto = Schema<typeof DiceRtrRollResponseSchema, SchemaValidator>;
export type DiceRtrStatsResponseDto = Schema<typeof DiceRtrStatsResponseSchema, SchemaValidator>;
```

And add the stats schema to `diceRtr.schema.ts`:

```typescript
import { number, record, string } from '@dice-roll-node-app/core';

// idiomatic validator schema defines the request schema. This should extend the request type
export const DiceRtrRequestSchema = {
    message: string
};

// idiomatic validator schema defines the response schema. This should extend the response type
export const DiceRtrResponseSchema = {
    message: string
};

// Request schema for rolling dice
export const DiceRtrRollRequestSchema = {
    dieType: string // e.g., "d4", "d6", "d12", "d20"
};

// Response schema for roll result
export const DiceRtrRollResponseSchema = {
    dieType: string,
    result: number,
    id: string,
    createdAt: string
};

// Response schema for stats
export const DiceRtrStatsResponseSchema = {
    totalRolls: number,
    byDieType: record(string, {
        count: number,
        average: number,
        min: number,
        max: number
    })
};
```

## Step 13: Set Up Database

### Start Docker Containers (optional)

```bash
docker-compose up -d postgres
```

### Initialize Migrations (starts docker containers)

```bash
pnpm migrate:init
```

### Create Migration

```bash
pnpm migrate:create
```

### Run Migrations

```bash
pnpm migrate:up
```

## Step 14: Build and Run

```bash
# Install dependencies
pnpm install

# Build the application
pnpm build

# Start the development server
pnpm dev
```

You should see:
```
INFO: 🎉 RollDiceSvc Server is running at http://0.0.0.0:8000 🎉
```

## Step 15: Test the API

### Roll a Dice

```bash
curl -X POST http://localhost:8000/dice-rtr/roll \
  -H "Content-Type: application/json" \
  -d '{"dieType": "d20"}'
```

**Response:**
```json
{
  "dieType": "d20",
  "result": 15,
  "id": "ab45bbcb-40f1-425e-9a42-495b4065bf1b",
  "createdAt": "2025-11-12T07:03:17.634Z"
}
```

### Get Statistics

```bash
curl http://localhost:8000/dice-rtr/stats
```

**Response:**
```json
{
  "totalRolls": 4,
  "byDieType": {
    "d20": {
      "count": 2,
      "average": 10.5,
      "min": 6,
      "max": 15
    },
    "d6": {
      "count": 1,
      "average": 4,
      "min": 4,
      "max": 4
    },
    "d12": {
      "count": 1,
      "average": 11,
      "min": 11,
      "max": 11
    }
  }
}
```

## Next Steps

- Add more endpoints (e.g., get roll history)
- Add authentication
- Deploy to production
- Explore the monitoring dashboard at `http://localhost:3000` (Grafana)

## Troubleshooting

**Port conflicts**: Change `PORT` in `.env.local`

**Database connection errors**: Ensure PostgreSQL is running:
```bash
docker-compose ps
docker-compose logs postgres
```

**Migration errors**: Check that migrations ran:
```bash
pnpm migrate:status
```

**Service not starting**: Check logs for missing environment variables or configuration errors.
