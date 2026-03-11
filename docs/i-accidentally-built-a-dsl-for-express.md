---
title: I Accidentally Built a DSL for Express
category: Blog
description: The journey of building a domain-specific language for modern backends on top of Express.
---

When I started this project, I didn't set out to build a DSL. I was curious about Typescript's expressiveness and the ergonomics of modern frameworks, like ElysiaJS after coming from other languages like C#, Python and Rust. Like many at my new job; I just used simple Express to power a bunch of endpoint. I had a problem I wanted to fix. What I really wanted was to rewrite the API layer to have DX niceties, but I couldn't justify it to my superiors. The reality was that those pieces had to be monkey patched, and I REALLY wanted to throw it all out. So I thought, how can I bring these features to Express without having to justify a rewrite to my boss? So; I started a side project.

That was the seed. But every time I solved one problem, another revealed itself. What started as a small utility has grown into something bigger: a domain-specific language for modern backends, sitting quietly on top of Express.

## Static Validation Inference

The journey began with schemas. I wanted request/response validation that was both runtime-checked and type-inferred, just like Elysia, but more explicit. By wiring Zod/TypeBox into routes, Express suddenly felt like a statically typed language instead of a loosely typed framework. At first, I thought I was done. But contracts opened the door to so much more: they were the foundation that every other piece would end up building on.

👉 [Graphic: vanilla Express vs. ForkLaunch contract with inferred types]

## Live Documentation via OpenAPI

This part was inspired directly by my days with FastAPI and also, ElysiaJS. They showed how powerful it is to get live documentation "for free." If the schemas were already there, why not emit OpenAPI automatically? Additionally, I really wanted to try the Scalar SDK. Swagger felt like the past. I realized however, that the zod and typebox shims actually were a very powerful construct for generating OpenAPI.

👉 [Gif: edit code → refresh Swagger → spec updates instantly]

## Correlated Telemetry

Then, the next piece I built came from pain, but also ideas I had seen in my early career. I've always hated debugging across logs, traces, and metrics; it feels like finding needles in haystacks. So I baked in the ability to track workflows of APIs through correlated telemetry: every request automatically produces logs, traces, and metrics that share the same ID. Debugging went from chasing ghosts to following a single thread of truth. What started as "for me" became something I couldn't imagine building without.

👉 [Diagram: one request → correlated log, trace, metric]

## A Simple DI Container

Telemetry revealed how messy my services were. Imports, singletons, tight coupling: it was unsustainable. So I built the simplest DI container I could: register objects with lifetimes (singleton, scoped, transient) and schemas for validation, and resolve them at runtime. No decorators, no magic; just a clean object access pattern. It just felt right.

## Dynamic SDK Clients

Schemas + OpenAPI naturally begged for SDKs. So I built a dynamic client generator: it coerces inputs/outputs, groups endpoints, and even allows renames. Suddenly, TypeScript autocomplete felt like using a polished SaaS SDK, but for my own backend. That was when I realized: ForkLaunch wasn't just about servers, it was about the developer experience of consuming them too.

👉 [Gif: editor autocomplete suggesting generated client methods]

## Auth, Versioning, and Rate Limiting

From there, the production concerns came fast. I didn't want users wiring auth, versioning and rate limiting manually, so I baked them into the contract layer. Declare it once, and it's enforced everywhere. It wasn't about innovation; it was about making the boring parts invisible.

## Clustered Hosting

Scaling was next. Node's cluster API has always existed, but nobody really integrates it cleanly. Asyncio from python also uses event loops, and I loved the uvicorn concurrency of multiple workers. So, I wanted clustering to be a built-in choice: one flag, and your app fans across CPU cores. Better yet, it runs the same on Node and Bun.

👉 [Graphic: clustered workers diagram]

## Live MCP and Simulated Handlers

Then came MCP (Model Context Protocol). With ForkLaunch contracts, generating live-typed MCP endpoints was natural: suddenly, AI agents could interact with APIs directly through strongly-typed contracts. To support this, I added simulated handlers for local testing: in-process mocks that behave exactly like production, with full type safety. That unlocked something I didn't expect: testing APIs without needing the network at all.

👉 [Gif: simulated handler tests running inline, no HTTP]

## The CLI

By now, managing everything by hand wasn't an option. The CLI emerged to scaffold projects, generate contracts, and spin up clusters. Over time it became more than a tool; it became the keeper of best practices.

## Greenfield and Brownfield

The most surprising part? ForkLaunch works in both worlds. Greenfield projects get contracts, telemetry, clustering, and MCP out of the box. But it also shines in brownfield: you can drop it into an existing Express codebase and upgrade routes incrementally. No rewrites, no painful migrations; just evolve, endpoint by endpoint. That's what makes this more than a framework. It's an upgrade path.

👉 [Gif: before/after Express route upgrade]

## Where I Want to Take This

Here's the part I'm most excited about. ForkLaunch today is an accidental DSL, but I see it evolving into a workflow engine for backends.

**Structured workflows**: Imagine declaring retry logic, compensating actions, or branching in a simple way, right on top of route definitions. Express routes wouldn't just handle requests; they'd orchestrate systems.

**Improved testing**: With simulated handlers, we can go further: generating synthetic data directly from contracts, so tests don't just assert; they explore. Imagine fuzzing APIs automatically, with type-safe, schema-valid data generated on the fly.

**Next-gen DX**: The long-term vision is that ForkLaunch makes backend development feel like Next.js did for frontend: declarative, incremental, and integrated with the surrounding ecosystem.

This started with type inference. It grew into telemetry, clustering, DI, MCP, and now a path toward workflows and synthetic testing. ForkLaunch wasn't planned; but maybe the best DSLs never are.

## What I Actually Built

Looking back, ForkLaunch now includes:

- Type-safe contracts
- Live OpenAPI docs (inspired by FastAPI, et al.)
- Correlated telemetry
- Lightweight DI
- Dynamic SDK clients
- Built-in auth + rate limiting
- Clustered hosting (Node + Bun)
- Live MCP support
- Simulated handlers for local testing
- A CLI for best practices
- Incremental adoption for Express apps
- A roadmap toward structured workflows + synthetic data testing

ForkLaunch has become a way to express modern backend concerns declaratively on top of Express.

I am also building a command line tool that is used by a couple early stage companies, like Frontera Claims and Martingales, that drops in common modules into existing codebases, but I'll talk about that in a future post.

