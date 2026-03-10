---
title: Contributing to ForkLaunch
category: Community
description: Guidelines and information for contributing to ForkLaunch projects.
---

Thank you for your interest in contributing to ForkLaunch! We welcome contributions from the community and appreciate your help in making our products better.

## Our Products

ForkLaunch consists of three main products, each with its own license and development process:

### 1. CLI (AGPL License)

- Command-line interface for ForkLaunch
- Located in `cli/` directory
- By contributing, you agree to license your code under AGPL

### 2. Blueprint (MIT License)

- Core blueprint functionality
- Located in `blueprint/` directory
- Focus on backwards compatibility

### 3. Framework (MIT License)

- Utility framework and extensions
- Located in `framework/` directory
- Rapid development cycle
- Emphasis on documentation

## Getting Started

### CLI Development (Rust)

1. Install Rust via [rustup](https://rustup.rs/)
2. Navigate to `cli/` directory
3. Run `cargo build`
4. Run tests with `cargo test`
5. Run E2E tests with scripts in `test/xxxxxx.sh`

### Blueprint & Framework Development (TypeScript)

1. Install dependencies: `pnpm install`
2. Build all framework: `pnpm build`
3. Run tests: `pnpm test`

All npm scripts for the Blueprint and Framework are available in the root `package.json`. The CLI has its own Rust-specific tooling in the `cli/` directory.

## Development Workflow

Before any development, review security guidelines in `cli/SECURITY`

### CLI Development

1. Make changes in `cli/` directory
2. Format code: `cargo fmt`
3. Run clippy: `cargo clippy`
4. Test: `cargo test`

### Blueprint & Framework Development

1. Make your changes
2. Format and lint: `pnpm format && pnpm lint`
3. Run tests: `pnpm test`
4. For specific package testing: `pnpm test --filter @forklaunch/package-name`

## Common Requirements

- Commit using [conventional commits](https://www.conventionalcommits.org/)
- Open a Pull Request

## Pull Request Guidelines

- Clearly indicate which product you're contributing to in the PR title
- Describe the changes you've made and why
- Reference any related issues
- Ensure all tests pass and there are no linting errors
- Keep changes focused and atomic - one feature/fix per PR

## Code Style

- We use TypeScript & Rust for type safety
- Follow the existing code style and patterns
- Use meaningful variable and function names
- Add JSDoc comments for public APIs
- Write unit tests for new functionality

## Getting Help

- Open an issue for bugs or feature requests
- Join our [Discord community](https://discord.gg/forklaunch) for questions
- Check the [documentation](https://docs.forklaunch.com)

## License Acknowledgment

By contributing to ForkLaunch products, you acknowledge and agree that:

- CLI contributions will be licensed under AGPL
- Blueprint and Framework contributions will be licensed under MIT

## Code of Conduct

Please note that this project is released with a [Contributor Code of Conduct](CODE_OF_CONDUCT). By participating in this project you agree to abide by its terms.
