# litelite

A kit for **purpose-sized languages** — the largest language for which your
guarantees stay mechanical.

Smallness is not a cost you pay for embeddability; it is what buys guarantees
big languages cannot give. Fuel-bounded evaluation is a **termination proof**.
A host-capability table is a **complete effect bound**. A byte budget is a
**hard output cap**. When agents write, trade, and execute programs, "it
compiles" is testimony — *provably halts within N fuel, provably touches only
these capabilities* is physics.

litelite is the shared kernel extracted from three production language subsets
(rustlite → wasm cartridges, soliditylite → EVM bytecode, bashlite → sandboxed
shell, ~19K LOC in [localharness](https://github.com/compusophy/localharness))
that each hand-rolled these pieces — with divergent bugs to show for it. The
kit pays each invariant exactly once.

## Crates

| crate | what | the invariant paid once |
|---|---|---|
| `diaglite` | spans, coded diagnostics, caret snippets | mid-char span offsets floor to a boundary instead of panicking |
| `lexlite` | byte-cursor lexer kit | UTF-8-safe char consumption; nested-vs-flat block comments are an explicit flag |
| `parselite` | recursive-descent harness | the depth guard is the only way in — deeply nested input returns a `Diag`, never a stack abort |
| `fuellite` | fuel + byte budgets | one shared budget across all composition — fractal recursion terminates by construction |
| `litelite` | facade | `cargo add litelite` re-exports the four |

Zero external dependencies. Native + `wasm32-unknown-unknown`.

```rust
use litelite::{diag::Diag, fuel::Fuel, lex::Cursor, parse::TokCursor};

let mut fuel = Fuel::new(10_000);       // every loop iteration burns —
fuel.burn(1)?;                          // the program halts, mechanically
```

## Status

Genesis (M0). The kernel is ported and tested; the first language built on it
(`prooflite`) is next. Roadmap and origin: [`GENESIS.md`](GENESIS.md).
Research plan: [`paper/OUTLINE.md`](paper/OUTLINE.md).

This repo is constitutionally small: ≤2,000 LOC per crate, ≤25,000 total,
CI-enforced (`scripts/caps.sh`). The two predecessor projects each became
unworkable near 120K LOC; this one cannot get there.

## Build

```sh
cargo test
cargo check --target wasm32-unknown-unknown
bash scripts/caps.sh
```

## License

Apache-2.0
