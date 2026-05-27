# Contributing

Created by Daniel Fat.

Dongler is a Rust-first project. Extraction behavior belongs in
`crates/dongler-core`; Python and TypeScript should stay as bindings and API
wrappers around that Rust implementation.

## Development

```bash
make test
```

Run focused checks while working:

```bash
make test-rust
make test-python
make test-js
```

## Guidelines

- Keep new document engines behind clear Rust traits.
- Add tests for every public behavior change.
- Publish only through the protected GitHub Actions release workflow.
- Keep licensing metadata as `MIT`.
