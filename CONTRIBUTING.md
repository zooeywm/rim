# Contributing

## First Principles

- Preserve behavior unless the change explicitly targets behavior.
- Keep `rim-domain` pure.
- Keep `rim-application` orchestration-focused.
- Keep `rim-app` thin.

Read [ARCHITECTURE.md](ARCHITECTURE.md) before non-trivial changes.

## Setup

```bash
cargo check -q
cargo test --workspace --no-run
cd docs
pnpm install
pnpm dev
```

## Where Code Goes

- Editor rules, cursor movement, text mutation, session reconstruction: `rim-domain`
- Actions, command handling, workbench state, config application: `rim-application`
- Traits for external capabilities: `rim-ports`
- Storage, watcher, input, and UI integrations: `rim-infra-*`
- Bootstrap and runtime loop: `rim-app`

If a change needs both pure logic and side effects, split it. Put the pure transition in `rim-domain` and call it from `rim-application`.

## Change Process

1. Start from the owning crate, not from the facade.
2. Keep public boundaries explicit.
3. Prefer small, compile-safe slices.
4. Run formatting and workspace verification before finishing.

Required verification:

```bash
rustfmt +nightly --edition 2024 $(git ls-files '*.rs')
cargo check -q
cargo test --workspace --no-run
```

## Documentation Expectations

Update docs when you change:

- crate responsibilities
- data flow
- persistence behavior
- config files
- contributor-facing extension points

Relevant docs:

- [ARCHITECTURE.md](ARCHITECTURE.md)
- [ADR/0001-layered-hexagonal-architecture.md](ADR/0001-layered-hexagonal-architecture.md)
- `docs/src/content/docs/`

## Anti-Patterns To Avoid

- Mixing status-bar or overlay behavior into `rim-domain`
- Parsing config inside infra crates
- Having adapters mutate domain state directly
- Leaving non-compiled migration files in the tree after ownership has moved

## Pull Request Standard

A good change set should answer:

- What behavior changed, if any?
- Which layer owns the change and why?
- Which tests or compile checks cover it?
- Which docs were updated?
