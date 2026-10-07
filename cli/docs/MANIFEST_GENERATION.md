# Release Manifest Generation Documentation

## Overview

The release manifest generation system creates a comprehensive JSON document that describes an application's deployment configuration, including services, workers, infrastructure requirements, and environment variables. This document is used by the Forklaunch platform to provision and deploy applications.

## High-Level Flow

The manifest generation process follows these steps:

1. **Git Metadata Detection** - Extract commit hash and branch information
2. **OpenAPI Specification Export** - Generate OpenAPI specs for all services
3. **Environment Variable Detection** - Discover and classify environment variables
4. **Runtime Dependency Detection** - Identify required infrastructure resources
5. **Manifest Generation** - Assemble all information into the final manifest
6. **Upload/Export** - Upload to platform or save locally (dry-run)

## Key Data Structures

### ReleaseManifest

The root structure containing all release information:

```rust
struct ReleaseManifest {
    schema_version: Option<String>,           // Schema version (e.g., "1.0.0")
    application_id: String,                    // Platform application ID
    application_name: Option<String>,         // Human-readable app name
    version: String,                           // Release version (e.g., "1.0.0")
    runtime: Option<Runtime>,                  // Node or Bun
    git_commit: String,                        // Git commit hash
    git_branch: Option<String>,                // Git branch name
    git_repository: Option<String>,            // Git repository URL
    timestamp: String,                          // ISO 8601 timestamp
    services: Vec<ServiceDefinition>,         // List of services and workers
    infrastructure: InfrastructureConfig,      // Infrastructure requirements
    environment_variables: Option<EnvironmentVariables>,  // Deprecated
    required_environment_variables: Option<Vec<EnvironmentVariableRequirement>>,  // Env var requirements
}
```

### EnvironmentVariableRequirement

Describes a required environment variable with its scope and component metadata:

```rust
struct EnvironmentVariableRequirement {
    name: String,                              // Variable name (e.g., "DB_HOST")
    scope: EnvironmentVariableScope,            // Application, Service, or Worker
    scope_id: Option<String>,                  // Service/worker name if scoped
    description: Option<String>,               // Usage description
    component: Option<EnvironmentVariableComponent>,  // Component inference metadata
}
```

### EnvironmentVariableComponent

Metadata about what type of component an environment variable references:

```rust
struct EnvironmentVariableComponent {
    type: EnvironmentVariableComponentType,    // Database, Cache, Service, Worker, etc.
    property: EnvironmentVariableComponentProperty,  // Host, Port, URL, ConnectionString, etc.
    target: Option<String>,                     // Target service/worker name (for Service/Worker types)
    path: Option<String>,                       // URL path component (for URL values)
    passthrough: Option<String>,                // Literal value to pass through (e.g., "8000" for PORT)
}
```

## Step-by-Step Process

### Step 1: Git Metadata Detection

**Function**: `get_git_commit()`, `get_git_branch()`

- Validates that the current directory is a git repository
- Extracts the current commit hash (full SHA)
- Extracts the current branch name
- These are included in the manifest for traceability

### Step 2: OpenAPI Specification Export

**Function**: `export_all_services()`

- Scans all service projects in the manifest
- Generates OpenAPI 3.0 specifications for each service
- Saves specs to `.forklaunch/openapi/{service-name}/openapi.json`
- Loads the generated specs into memory for inclusion in the manifest

### Step 3: Environment Variable Detection

This is the most complex step, involving multiple sub-processes:

#### 3.1: Code-Based Discovery

**Function**: `find_all_env_vars()`

- Scans TypeScript/JavaScript files in the codebase
- Looks for environment variable usage patterns (e.g., `process.env.VAR_NAME`)
- Returns a map of `project_name -> Vec<EnvVarUsage>`

#### 3.2: Scope Determination

**Function**: `determine_env_var_scopes()`

Determines the scope (Application, Service, or Worker) for each discovered variable:

- **Application scope**: Variable is used by multiple projects, or by "core" or "monitoring"
- **Service scope**: Variable is used by exactly one service project
- **Worker scope**: Variable is used by exactly one worker project

The result is a `Vec<ScopedEnvVar>` with scope and scope_id information.

#### 3.3: Docker Compose Analysis

**Function**: `build_env_var_component_map()`

This function performs two key operations:

1. **Component Inference**: Analyzes docker-compose.yaml to infer component types and properties for environment variables
2. **Environment Variable Collection**: Collects all environment variables from docker-compose services

**Component Inference Logic**:

The function iterates through all services in docker-compose and for each environment variable, it attempts to infer:

- **Component Type**: What kind of component this variable references (Database, Cache, Queue, Service, Worker, Key, ObjectStore)
- **Component Property**: What property of that component (Host, Port, URL, ConnectionString, etc.)
- **Target**: If it's a Service/Worker reference, which service/worker it targets
- **Path**: If it's a URL, the path component
- **Passthrough**: If it's a literal value that should be passed through (e.g., PORT="8000")

**Inference Priority** (checked in order):

1. **Special Cases**:
   - `OTEL_EXPORTER_OTLP_ENDPOINT` → Service type, target="otel"
   - CLI-generated keys (`HMAC_SECRET_KEY`, `BETTER_AUTH_SECRET`) → Key type with specific properties

2. **Database Detection**:
   - Key hints: `DB_`, `DATABASE`, `POSTGRES`, `MYSQL`, etc.
   - Value hints: `postgres`, `mysql`, `postgresql`, etc.
   - Image hints: Docker image contains `postgres`, `mysql`, etc.

3. **Cache Detection**:
   - Key hints: `REDIS`, `CACHE`, `MEMCACHE`
   - Value hints: `redis`, `cache`, `memcache`
   - Image hints: `redis`, `memcache`

4. **Object Store Detection**:
   - Key hints: `S3`, `MINIO`, `OBJECT`, `BUCKET`
   - Value hints: `s3`, `minio`, `object`, `bucket`
   - Image hints: `minio`, `localstack`

5. **Queue Detection**:
   - Key hints: `QUEUE`, `KAFKA`, `BROKER`, `RABBITMQ`
   - Value hints: `queue`, `kafka`, `broker`, `rabbitmq`
   - Image hints: `kafka`, `rabbitmq`, `nats`

6. **Service/Worker Reference Detection**:
   - If the value is a URL (starts with `http://` or `https://`)
   - Extract the hostname from the URL
   - Match hostname against known service/worker identifiers (from docker-compose service names, hostnames, container names)
   - If matched, infer Service or Worker type with target set to the canonical service/worker name

7. **Service/Worker Port-Based Detection**:
   - If the service has a port in the 8000-8999 range (generated app port)
   - And the value is URL-like
   - Infer Service or Worker type based on the service's project type

8. **Passthrough Detection**:
   - If the value is not URL-like
   - And not a key-like variable
   - And doesn't contain KEY, TOKEN, SECRET, PASSWORD, ACCESS, PORT
   - And is not `NODE_ENV`
   - Mark as passthrough with the literal value

**Worker Alias Handling**:

Workers in docker-compose can have multiple service definitions:
- `{worker-name}-server` or `{worker-name}-service` → Service component type
- `{worker-name}-worker` → Worker component type

The `classify_worker_alias()` function detects these patterns and maps them to the base worker name.

#### 3.4: Docker Compose Environment Variable Inclusion

After building the component map, the system:

1. Collects all environment variables from docker-compose services
2. For each service/worker in docker-compose:
   - Determines if it's a direct project match or a worker alias
   - For worker aliases, determines the appropriate scope (Service or Worker) and base worker name
   - Adds all env vars from that service to `scoped_env_vars` with the correct scope
   - Special handling: `PORT` variables get passthrough value "8000"

3. Prevents duplicates by tracking `(variable_name, scope_id)` pairs

#### 3.5: Application Variable Filtering

**Function**: `is_allowed_application_var()`

Application-scoped variables are filtered to only include:
- Service/Worker URLs (variables with Service or Worker component type and Url property)
- HMAC keys (variable name contains "HMAC")
- JWKS Public Keys (variable name contains "JWKS", "PUBLIC", and "KEY")

This prevents component-specific variables (like `QUEUE_NAME`) from being incorrectly scoped as application-wide.

### Step 4: Runtime Dependency Detection

**Function**: `find_all_runtime_deps()`

- Scans the codebase for runtime dependency declarations
- Groups dependencies by project
- Converts to resource types (database, cache, queue, object_store)
- Filters out "monitoring" as it's not provisionable
- Returns a map of `project_name -> Vec<resource_type>`

### Step 5: Integration Detection

**Function**: `find_all_integrations()`

- Scans `registrations.ts` files for `runtimeDependencies` declarations
- Extracts integration information from dependency registrations
- Maps dependency names to integration types:
  - **Database**: `MikroORM`, `EntityManager` → `"Database"`
  - **Cache**: `RedisClient`, `Redis`, `RedisCache`, `TtlCache` → `"Cache"`
  - **ObjectStore**: `S3ObjectStore`, `S3Client`, `S3` → `"ObjectStore"`
  - **MessageQueue**: `KafkaClient`, `Kafka`, `BullMQ`, `QueueClient` → `"MessageQueue"`
  - **Observability**: `OpenTelemetryCollector`, `PrometheusClient` → `"Observability"`
  - **ThirdParty**: Any unknown dependency → `"ThirdParty"`
- Filters out non-integration dependencies (e.g., `SchemaValidator`, `Metrics`, `WorkerOptions`, `WorkerConsumer`, `WorkerProducer`)
- Generates integration IDs in Title Case with spaces (e.g., `"EntityManager"` → `"Entity Manager"`, `"S3ObjectStore"` → `"S3 Object Store"`)
- IDs do not include the integration type suffix
- Returns a map of `project_name -> Vec<Integration>`

Each `Integration` contains:
- `id`: Unique identifier in Title Case with spaces (e.g., `"Entity Manager"`, `"S3 Object Store"`, `"Redis Client"`)
- `integration_type`: Type of integration (`"Database"`, `"Cache"`, `"ObjectStore"`, `"MessageQueue"`, `"Observability"`, `"ThirdParty"`)
- `config`: Configuration map (currently empty, can be populated with additional metadata)

### Step 6: Manifest Generation

**Function**: `generate_release_manifest()`

Assembles all collected information into the final manifest:

1. **Services Array**:
   - For each Service project: Creates `ServiceDefinition` with `ServiceConfig`
   - For each Worker project: Creates `ServiceDefinition` with `WorkerConfig`
   - Includes OpenAPI specs for services
   - Includes runtime dependencies
   - Includes integrations (for services only)
   - Determines build context and dockerfile paths
   
   **Integrations**: For each service, integrations are converted from `Integration` structs to `IntegrationDefinition` objects with:
   - `id`: Integration identifier
   - `integration_type`: Type of integration
   - `config`: Configuration map
   - `status`: Optional status (currently `None`)

2. **Infrastructure Config**:
   - Extracts resources from project resource inventories
   - Sets default cloud provider (AWS)
   - Sets build context if Dockerfile exists

3. **Environment Variables**:
   - Converts `ScopedEnvVar` list to `EnvironmentVariableRequirement` list
   - Maps component metadata from the component map
   - Includes scope, scope_id, description, and component information

4. **Metadata**:
   - Sets schema version
   - Includes git information
   - Sets timestamp to current UTC time

### Step 7: Upload/Export

**Function**: `upload_release()` or file write (dry-run)

- If not dry-run: Uploads manifest to platform API
- If dry-run: Writes manifest to `.forklaunch/release-manifest.json`
- Updates application manifest with release version and git info

## Component Type Inference Details

### Component Types

- **Database**: PostgreSQL, MySQL, MariaDB, MongoDB, etc.
- **Cache**: Redis, Memcache
- **ObjectStore**: S3, MinIO
- **Queue**: Kafka, RabbitMQ, NATS, BullMQ
- **Service**: References to other services in the application
- **Worker**: References to workers in the application
- **Key**: Encryption keys, HMAC secrets, etc.

### Component Properties

- **Host/Hostname**: Network hostname
- **Port**: Network port number
- **URL**: Full URL with scheme
- **Connection/ConnectionString**: Connection string or connection info
- **User/Username**: Authentication username
- **Password**: Authentication password
- **Database/DbName**: Database name
- **Bucket**: Object store bucket name
- **Endpoint**: API endpoint
- **Region**: Cloud region
- **Fqdn**: Fully qualified domain name
- **Key Material Variants**: PrivatePem, PublicPem, Base64Bytes32, Base64Bytes64, HexKey, KeyMaterial

### Property Inference

The `infer_component_property()` function analyzes the variable name (split by underscores) to determine the property:

- `*_HOST*` → Host
- `*_PORT*` → Port
- `*_URL*` or `*_URI*` → Url
- `*_CONNECTIONSTRING*` or `*_CONNECTION_STRING*` → ConnectionString
- `*_CONNECTION*` → Connection
- `*_USERNAME*` → Username
- `*_USER*` → User
- `*_PASSWORD*` → Password
- `*_DATABASE*` or `*_DB*` → Database
- `*_DATABASE_NAME*` or `*_DB_NAME*` → DbName
- `*_BUCKET*` → Bucket
- `*_ENDPOINT*` → Endpoint
- `*_REGION*` → Region
- `*_FQDN*` → Fqdn
- `*_PATH*` → Endpoint
- `*_BROKER*` or `*_QUEUE*` → Connection
- `*_SECRET*` or `*_TOKEN*` → Password
- `*_ACCESS_KEY*` → User
- `*_CLIENT*` → User

### Service/Worker Reference Detection

When an environment variable value is a URL, the system:

1. Parses the URL to extract the hostname
2. Looks up the hostname in a service lookup map built from docker-compose:
   - Service names
   - Container names
   - Hostnames
   - For workers: base worker name
3. If found, infers Service or Worker component type with target set to the canonical name

### Passthrough Values

Passthrough values are literal strings that should be passed through as-is. Examples:
- `PORT=8000` → passthrough="8000"
- `NODE_ENV=production` → NOT passthrough (filtered out)
- Simple configuration values that don't reference components

## Worker Alias Handling

Workers can have multiple docker-compose service definitions:

- **Server/Service Component**: `{worker-name}-server` or `{worker-name}-service`
  - Creates Service-scoped environment variables
  - scope_id = base worker name
- **Worker Component**: `{worker-name}-worker`
  - Creates Worker-scoped environment variables
  - scope_id = base worker name

This allows the same worker to have different environment variable scopes for its server and worker components.

## Special Cases

### PORT Variable

All `PORT` environment variables are automatically set with passthrough value "8000", regardless of their actual value in docker-compose.

### OTEL_EXPORTER_OTLP_ENDPOINT

Always inferred as:
- Type: Service
- Target: "otel"
- Property: Endpoint (or default)

### NODE_ENV

Explicitly excluded from passthrough classification (not marked as passthrough).

### CLI-Generated Keys

Only these specific variables are classified as Key component type:
- `HMAC_SECRET_KEY`
- `BETTER_AUTH_SECRET`

Other key-like variables are not automatically classified as Key type unless they match other inference rules.

## File Locations

- **Main Logic**: `cli/src/release/create.rs`
- **Manifest Structures**: `cli/src/release/manifest_generator.rs`
- **Scope Determination**: `cli/src/core/env_scope.rs`
- **Docker Compose Parsing**: `cli/src/core/docker.rs`
- **Environment Variable Discovery**: `cli/src/core/ast/infrastructure/env.rs`
- **Integration Discovery**: `cli/src/core/ast/infrastructure/integrations.rs`

## Example Manifest Structure

```json
{
  "schemaVersion": "1.0.0",
  "applicationId": "app-123",
  "applicationName": "my-app",
  "version": "1.0.0",
  "runtime": "node",
  "gitCommit": "abc123...",
  "gitBranch": "main",
  "timestamp": "2024-01-01T00:00:00Z",
  "services": [
    {
      "id": "my-service",
      "name": "my-service",
      "config": {
        "type": "service",
        "openApiSpec": { ... },
        "runtimeDependencies": ["database", "cache"],
        "integrations": [
          {
            "id": "Mikro ORM",
            "type": "Database",
            "config": {},
            "status": null
          },
          {
            "id": "Redis Client",
            "type": "Cache",
            "config": {},
            "status": null
          }
        ]
      }
    }
  ],
  "infrastructure": {
    "regions": [],
    "resources": [
      {
        "id": "my-service-db",
        "type": "postgresql",
        "name": "my-service-database"
      }
    ]
  },
  "requiredEnvironmentVariables": [
    {
      "name": "DB_HOST",
      "scope": "service",
      "scopeId": "my-service",
      "description": "Used by: my-service",
      "component": {
        "type": "database",
        "property": "host",
        "target": null,
        "path": null,
        "passthrough": null
      }
    }
  ]
}
```

## Key Functions Reference

- `build_env_var_component_map()`: Analyzes docker-compose and infers component metadata
- `infer_component_details()`: Determines component type, property, target, path, passthrough for a single env var
- `infer_component_type()`: Determines the component type based on hints
- `infer_component_property()`: Determines the property based on variable name
- `classify_worker_alias()`: Detects and classifies worker aliases
- `should_passthrough()`: Determines if a variable should be marked as passthrough
- `is_allowed_application_var()`: Filters application-scoped variables
- `determine_env_var_scopes()`: Determines scope for variables found in code
- `generate_release_manifest()`: Assembles the final manifest structure

