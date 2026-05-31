# agentmemory bundle (generated artifacts)

This directory holds pinned metadata for the Locus agentmemory offline bundle.

Run from the repo root:

```bash
bun run codegraph:bundle
bun run agentmemory:bundle
```

Build output (not committed):

- `node_modules/` — `@agentmemory/agentmemory@0.9.24` and dependencies
- `bin/iii` or `bin/iii.exe` — iii-engine v0.11.2
- `manifest.json` — generated bundle manifest

Runtime uses the Node binary from `../codegraph-bundle/` to launch
`node_modules/@agentmemory/agentmemory/dist/cli.mjs`.
