---
title: Plan Chat
category: Guides
description: Use AI-powered planning and operations chat to design architecture and manage infrastructure.
---

## Overview

Plan Chat is ForkLaunch's AI assistant, available per-application in your dashboard. It provides two distinct modes: **Planning** for designing project architecture, and **Operations** for managing deployed infrastructure, each in threaded conversations with full history.

## Accessing Plan Chat

Navigate to any application in your dashboard and click the **Chat** tab. This opens the Plan Chat interface with a thread sidebar on the left and the conversation area on the right.

> Plan Chat requires the **AI Chat** feature to be enabled on your billing plan.

## Modes

### Planning Mode

Planning mode helps you design your application architecture. Describe what you want to build and the AI will help you:

- Design service architecture and project structure
- Create step-by-step implementation plans
- Generate ForkLaunch CLI commands for scaffolding
- Recommend infrastructure choices (databases, caches, queues)
- Suggest patterns for service-to-service communication

When a plan is generated, you can **copy** or **export** it as a Markdown file using the buttons in the thread header.

### Operations Mode

Operations mode lets you interact with your deployed infrastructure. The AI can:

- Deploy and manage services
- Check service and deployment statuses
- Manage environment variables
- Roll back deployments
- Execute infrastructure operations directly

## Threads

Conversations are organized into threads. Each thread has:

- A **title** you provide when creating it
- A **mode** (planning or operations) set at creation time
- Full **message history** that persists across sessions
- **Timestamps** and creator attribution for team visibility

### Creating a Thread

Click **Plan** or **Chat** at the top of the sidebar to create a new thread. Give it a descriptive title and press Enter or click Create.

### Managing Threads

- Click any thread in the sidebar to load its conversation
- Hover over a thread to reveal the delete button
- Threads are sorted by most recently updated

## Streaming Responses

Responses stream in real-time as the AI processes your request. You'll see:

- **Text output** appearing progressively in a terminal-style block
- **Tool calls** shown as expandable cards with their arguments and results
- A **typing indicator** while waiting for the AI to begin responding

## Replaying Messages

Hover over any of your messages to reveal the **replay** button. This replays the conversation from that point, useful for iterating on a different approach without starting a new thread.

## Exporting Plans

In planning mode, when the AI generates a structured plan (with headings and sections), export options appear in the thread header:

- **Copy Plan** copies the plan markdown to your clipboard
- **Export** downloads the plan as a `.md` file named after your thread

## Tips

- Be specific about your requirements in planning mode: mention the number of services, infrastructure needs, and communication patterns
- Use operations mode for day-to-day infrastructure management rather than manual CLI commands
- Create separate threads for different topics to keep conversations focused
- Replay from earlier messages to explore alternative architectures without losing context
