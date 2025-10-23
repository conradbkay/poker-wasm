# Setup

The original Typescript implementation has tests. This module is tested by comparing it's output to the original implementation's

First add a HandRanks.dat file to the project root, which you can download here: <https://github.com/chenosaurus/poker-evaluator/blob/master/data/HandRanks.dat>

Simply run npm i then npm run main in the `ts` dir

## Test

```bash
cargo test
```

## Benchmark

Run all benchmarks:

```bash
cargo bench
```

## Publishing to npm

### Prerequisites

1. Ensure you have [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) installed:

```bash
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

2. Make sure you're logged in to npm:

```bash
npm login
```

### Publish New Version

To publish an updated version:

1. Update the version in `Cargo.toml`
2. Rebuild and publish:

```bash
wasm-pack build --target nodejs && cd pkg && npm publish
```
