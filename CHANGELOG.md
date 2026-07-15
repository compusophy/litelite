# Changelog

Every release is one version across every crate: they are one kit, and a
matrix of independently drifting versions is exactly the surface this repo
exists to refuse.

## 0.1.0 — 2026-07-15

The first published release. Eleven crates, **zero external dependencies**,
native + `wasm32-unknown-unknown`, ~8.3K LOC including tests.

Pre-1.0: the APIs are honest but young, and the kit has exactly one external
consumer so far. Expect breaking changes in 0.2 as re-homing a parent
(`bashlite`, M5) meets reality.

### The kernel — invariants paid once (M0)

- **`diaglite`** — `Span`, coded `Diag`, caret snippets. Mid-char span offsets
  floor to a boundary instead of panicking.
- **`lexlite`** — byte-cursor lexer kit. UTF-8-safe char consumption;
  nested-vs-flat block comments are an explicit flag, not an accident.
- **`parselite`** — recursive-descent harness where the depth guard is the
  only way in: deeply nested input returns a `Diag`, never a stack abort.
- **`fuellite`** — `Fuel` + `ByteBudget`. One shared budget across all
  composition, so fractal recursion terminates by construction.
- **`litelite`** — the facade; `cargo add litelite` re-exports the kit.

### Capabilities as data (M2)

- **`caplite`** — `Cap`/`CapTable`: one declarative table drives type
  checking, import ordering, human docs, and a versioned parity manifest with
  an FNV-1a-64 fingerprint a boundary's far side can recompute. Table drift
  becomes a red build instead of a runtime mystery.

### The emitters — independent by construction (M3)

- **`evmlite`** — EVM assembler (two-pass label back-patch, minimal-width
  push, init wrapper) plus a dependency-free EVM-subset interpreter as its
  diff-oracle: real JUMPDEST analysis, revert rollback, hard step/memory/stack
  bounds. Build faults are sticky errors — never wrong-but-clean bytecode.
- **`modlite`** — wasm binary module builder: LEB128, section framing,
  functype interning, locals RLE. The import-after-function index shift is a
  structural error, not a footgun.

These two share no abstraction and never will: wasm is structured and
relative, the EVM is absolute-jump. Trying to unify them is what the parents
proved doesn't work.

### The languages (M1, M4)

- **`prooflite`** — the reference total language. Every program halts within
  its fuel, output is byte-bounded, and its capability table is a complete
  effect bound (hostless runs provably touch nothing).
- **`stratlite`** — a total, fuel-bounded trading-strategy language. Every
  per-bar decision halts within its fuel; look-ahead is *grammatically
  unrepresentable* and pinned by a prefix-invariance test.
- **`backtestlite`** — stratlite's verifier: an all-integer deterministic
  engine where a whole backtest is one reproducible hash, and `verify()` is
  the generate→verify→keep predicate (compile + halt + survive + trade).

### Known limits

- `evmlite`'s oracle answers `KECCAK256` with `Unsupported`: hashing needs a
  dependency and the kit takes none. Its `MUL`/`DIV`/`MOD` are 128-bit, not a
  full 256-bit ALU — documented, not hidden.
- `modlite` covers type/import/function/memory/export/code/data sections only.
  No tables, globals, element, or start sections until a consumer needs them.
- `stratlite` has no position sizing, no intrabar fills, and no host seam.
- The paper's §5 experiment (generate strategies with a model, select
  survivors) is not in this repo: the instrument shipped, the run needs a
  model and real market data.
