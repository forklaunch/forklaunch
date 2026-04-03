{{#is_node}}FROM node:23-alpine{{/is_node}}{{#is_bun}}FROM oven/bun:1{{/is_bun}}

# Set working directory
WORKDIR /{{app_name}}

# Copy source code
COPY . .
{{#is_node}}{{#is_in_memory_database}}
# Install sqlite dependencies
RUN apk add --no-cache python3 py3-pip make build-base sqlite-dev
RUN pip install setuptools --break-system-packages
{{/is_in_memory_database}}{{/is_node}}
# Install dependencies
{{#is_node}}RUN npm install -g pnpm
RUN apk update
RUN apk add --no-cache libc6-compat
RUN apk add --no-cache git
RUN apk add --no-cache curl
RUN pnpm install{{/is_node}}{{#is_bun}}RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*
RUN bun install --ignore-scripts{{/is_bun}}

# Build TypeScript code
{{#is_node}}
RUN pnpm build{{/is_node}}{{#is_bun}}
RUN bun run build{{/is_bun}}
