# Skill: Production Database Query

## When to Use This Skill
- User asks to query, update, or inspect the production database
- User needs to check or modify records in platform-management tables (e.g., `organization_infrastructure`, deployments, applications)
- User says "check the DB", "query the database", "update the record", etc.

## Overview

The production database is an RDS PostgreSQL instance in a private subnet. Direct connections from local machines are not possible. Instead, run a one-off ECS Fargate task using a `postgres:16-alpine` container to execute `psql` commands.

## Prerequisites

- AWS CLI configured with appropriate credentials
- Access to the forklaunch ECS cluster in us-west-2

## Connection Details

- **Cluster**: `forklaunch-production-us-west-2-0aef7f56-cluster-9c2edb96`
- **Region**: `us-west-2`
- **Subnets** (private, same VPC as RDS): `subnet-00e7a3e035420558b`, `subnet-0ccf2a8c133601af3`
- **Security Group**: `sg-0922dd814f3377845`
- **RDS Host**: `forklaunch-production-us-west-2-0aef7f56-58c2c67490140ff.cja84sii63zr.us-west-2.rds.amazonaws.com`
- **User**: `dbadmin`
- **Port**: `5432`
- **SSL**: `sslmode=require`

## Databases

| Database | Module |
|---|---|
| `platform_management_database` | platform-management (orgs, apps, deployments, infrastructure) |
| `iam_database` | IAM (users, sessions, auth) |
| `billing_database` | Billing |
| `developer_tools_database` | Developer tools |
| `observability_api_database` | Observability API |
| `resource_management_database` | Resource management |
| `shared_postgres` | Shared/default (usually empty) |

## Step-by-Step

### 1. Register a temporary task definition (first time only)

Get the execution role from an existing service:

```bash
EXEC_ROLE=$(aws ecs describe-task-definition \
  --task-definition "$(aws ecs list-task-definitions --family-prefix aaa202ed --region us-west-2 --query 'taskDefinitionArns[0]' --output text)" \
  --region us-west-2 --query 'taskDefinition.executionRoleArn' --output text)

TASK_ROLE=$(aws ecs describe-task-definition \
  --task-definition "$(aws ecs list-task-definitions --family-prefix aaa202ed --region us-west-2 --query 'taskDefinitionArns[0]' --output text)" \
  --region us-west-2 --query 'taskDefinition.taskRoleArn' --output text)
```

Register the task definition:

```bash
aws ecs register-task-definition \
  --family "db-query-tool" \
  --requires-compatibilities FARGATE \
  --network-mode awsvpc \
  --cpu "256" \
  --memory "512" \
  --execution-role-arn "$EXEC_ROLE" \
  --task-role-arn "$TASK_ROLE" \
  --container-definitions '[{
    "name": "psql",
    "image": "postgres:16-alpine",
    "essential": true,
    "logConfiguration": {
      "logDriver": "awslogs",
      "options": {
        "awslogs-group": "/ecs/aaa202ed-473c-45e2-ab41-a789ad109986-forklaunch-production-us-west-2-0aef7f56-application-9c2edb96",
        "awslogs-region": "us-west-2",
        "awslogs-stream-prefix": "db-query"
      }
    }
  }]' \
  --region us-west-2
```

### 2. Run a query

Replace `<DATABASE>` with the target database and `<SQL>` with your query:

```bash
TASK_ARN=$(aws ecs run-task \
  --cluster forklaunch-production-us-west-2-0aef7f56-cluster-9c2edb96 \
  --task-definition "db-query-tool" \
  --launch-type FARGATE \
  --network-configuration '{"awsvpcConfiguration":{"subnets":["subnet-00e7a3e035420558b"],"securityGroups":["sg-0922dd814f3377845"],"assignPublicIp":"DISABLED"}}' \
  --overrides "{
    \"containerOverrides\": [{
      \"name\": \"psql\",
      \"command\": [\"psql\", \"postgresql://dbadmin:<PASSWORD>@forklaunch-production-us-west-2-0aef7f56-58c2c67490140ff.cja84sii63zr.us-west-2.rds.amazonaws.com:5432/<DATABASE>?sslmode=require\", \"-c\", \"<SQL>\"]
    }]
  }" \
  --region us-west-2 \
  --query 'tasks[0].taskArn' --output text)
```

### 3. Wait for results and read logs

```bash
# Extract task ID from ARN
TASK_ID=$(echo "$TASK_ARN" | awk -F/ '{print $NF}')

# Wait for task to complete
aws ecs wait tasks-stopped \
  --cluster forklaunch-production-us-west-2-0aef7f56-cluster-9c2edb96 \
  --tasks "$TASK_ID" --region us-west-2

# Read output
aws logs get-log-events \
  --log-group-name "/ecs/aaa202ed-473c-45e2-ab41-a789ad109986-forklaunch-production-us-west-2-0aef7f56-application-9c2edb96" \
  --log-stream-name "db-query/psql/$TASK_ID" \
  --region us-west-2 \
  --query 'events[*].message' --output text
```

### 4. Clean up (after done with all queries)

```bash
aws ecs deregister-task-definition --task-definition db-query-tool:<REVISION> --region us-west-2
```

## Key Tables

### `organization_infrastructure` (platform_management_database)
Stores VPC IDs per org per region. Used by deployment processor to determine `existingVpcId`.

```sql
SELECT id, organization_id, region, vpc_id FROM organization_infrastructure;
```

### Common queries

```sql
-- Check VPC records for an org
SELECT * FROM organization_infrastructure WHERE organization_id = '<ORG_ID>';

-- Update VPC ID for a region
UPDATE organization_infrastructure SET vpc_id = '<VPC_ID>', updated_at = NOW()
WHERE organization_id = '<ORG_ID>' AND region = '<REGION>';

-- Insert VPC ID for a new region
INSERT INTO organization_infrastructure (id, organization_id, region, vpc_id, created_at, updated_at)
VALUES (gen_random_uuid(), '<ORG_ID>', '<REGION>', '<VPC_ID>', NOW(), NOW());
```

## Important Notes

- **Always use `sslmode=require`** — RDS enforces SSL
- **Ask the user for the DB password** — do not store it in this skill file
- **Quote single quotes carefully** in ECS command overrides — use `'"'"'` for shell escaping within JSON
- **Each query is a new ECS task** — takes ~30-60s to provision Fargate, run, and return logs
- **Deregister task definitions when done** to avoid clutter
- **The password may change** — if queries fail with auth errors, ask the user for the current password
