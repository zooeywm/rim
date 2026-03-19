# ADR 0001: Layered Hexagonal Architecture

- Status: Accepted
- Date: 2026-03-19

## Context

The original workspace concentrated editor model, use-case orchestration, ports, and runtime-facing state in one central crate. That shape made the runtime convenient, but it obscured ownership:

- pure editor rules and workbench state were mixed
- adapters depended on central internal types instead of stable boundaries
- the composition root still contained application behavior
- contributor guidance for where new code should go was weak

The repository is intended to support ongoing editor growth, recovery features, and multiple runtime adapters. The architecture needs explicit ownership boundaries.

## Decision

Adopt and finalize a layered, hexagonal workspace:

- `rim-domain` owns the pure editor core
- `rim-application` owns use-case orchestration and workbench state
- `rim-ports` owns outbound contracts
- `rim-infra-*` owns adapters
- `rim-app` owns composition and runtime shell

```mermaid
flowchart TD
    Domain["rim-domain"]:::domain
    Application["rim-application"]:::application
    Ports["rim-ports"]:::ports
    Infra["rim-infra-*"]:::infra
    App["rim-app"]:::app

    Application --> Domain
    Application --> Ports
    Infra --> Application
    Infra --> Domain
    Infra --> Ports
    App --> Application
    App --> Infra

    classDef domain fill:#d8f3dc,stroke:#2d6a4f,color:#081c15;
    classDef application fill:#fff3bf,stroke:#8d6e00,color:#3d2f00;
    classDef ports fill:#dceeff,stroke:#1d4ed8,color:#0f172a;
    classDef infra fill:#ffe3e3,stroke:#c92a2a,color:#3f0000;
    classDef app fill:#ede7f6,stroke:#5f3dc4,color:#240046;
```

## Consequences

### Positive

- Pure editor logic is testable without runtime infrastructure.
- Workbench behavior stays explicit instead of leaking into the domain.
- Adapters bind to ports and stable application/domain types.
- The composition root is easier to reason about.
- Contributor guidance maps directly to crate ownership.

### Negative

- More cross-crate imports must be maintained deliberately.
- Developers must think about ownership before placing code.

## Rules Derived From This ADR

- `rim-domain` must not depend on any workspace crate.
- `rim-domain` must not own notifications, picker state, config state, or terminal concerns.
- `rim-application` must orchestrate use cases instead of duplicating domain logic.
- `rim-app` must not absorb testable application logic.
- `rim-ports` must remain traits only.

## Outcome

The migration is complete. The repository now uses the final crate graph directly.

## Follow-Up

- Continue tightening application wrappers that still mainly delegate to domain methods.
- Keep architecture docs aligned with the crate graph.
- Add CI checks that guard crate dependency direction if the workspace grows.
