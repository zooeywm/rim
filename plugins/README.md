# Plugins

This directory is the placeholder workspace plugin root used by the v1 WASM plugin skeleton.

Expected layout for a future plugin:

```text
plugins/
  my-plugin/
    plugin.toml
    plugin.wasm
```

Minimal manifest shape consumed by the current discovery skeleton:

```toml
id = "my-plugin"
name = "My Plugin"
version = "0.1.0"
abi_version = 1
entry = "plugin.wasm"
declared_capabilities = ["command_provider"]

[[commands]]
id = "echo"
title = "Echo"
description = "Placeholder command metadata"
```
