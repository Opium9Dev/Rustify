# Building

## Development

```powershell
cargo run
```

## Checks

```powershell
cargo fmt --check
cargo check --locked
```

## Windows release

```powershell
scripts\publish.cmd
```

The script builds an optimized release without embedding absolute source paths in compiler metadata. The executable is copied to `dist\rustify.exe`.
