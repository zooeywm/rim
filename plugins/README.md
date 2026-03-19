# Plugins

This directory stores official Rim plugin source crates.

Current layout:

```text
plugins/
  rim-plugin-yazi/
```

Runtime discovery now reads `.wasm` plugin components from the user config directory:

```text
$XDG_CONFIG_HOME/rim/plugins/*.wasm
```

On macOS and Windows this maps to the existing `rim-paths::user_config_root()` platform-specific config
directory.

Build the bundled yazi plugin artifact with:

```bash
rustup target add wasm32-wasip2
cargo build -p rim-plugin-yazi --target wasm32-wasip2
```

Then copy the resulting `.wasm` file into the user config plugin directory, for example:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/rim/plugins"
cp target/wasm32-wasip2/debug/rim_plugin_yazi.wasm "${XDG_CONFIG_HOME:-$HOME/.config}/rim/plugins/"
```

After that, startup discovery will load the plugin through the normal application/runtime path.
