# rim

## Overview

`rim` is a terminal-first editor prototype built around a state-driven Rust architecture, with rope-based text storage, workspace session restore, swap recovery, and persistent undo/redo.

## Installation

```bash
cargo install --path rim-app --locked
```

## Documentation

Full documentation is published at [zooeywm.github.io/rim](https://zooeywm.github.io/rim/).

To run the docs locally:

```bash
cd docs
pnpm install
pnpm dev
```

## Quick Start

```bash
cargo run -p rim-app --
cargo run -p rim-app -- path/to/file.rs
```
