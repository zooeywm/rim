# Plugins

This directory is the workspace plugin root used by the v1 WASM CommandProvider implementation.

Development layout:

```text
plugins/
  example/
    plugin.toml
```

The checked-in example manifest points to the workspace example plugin crate build output:

```toml
id = "example"
entry = "../../target/wasm32-wasip2/debug/rim_plugin_example.wasm"
```

Build the example plugin artifact with:

```bash
rustup target add wasm32-wasip2
cargo build -p rim-plugin-example --target wasm32-wasip2
```

After that, startup discovery will load the example plugin through the normal application/runtime path.
