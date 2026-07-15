# litelite

[![crates.io](https://img.shields.io/crates/v/litelite.svg)](https://crates.io/crates/litelite)
[![docs.rs](https://docs.rs/litelite/badge.svg)](https://docs.rs/litelite)
[![ci](https://github.com/compusophy/litelite/actions/workflows/ci.yml/badge.svg)](https://github.com/compusophy/litelite/actions/workflows/ci.yml)

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
| `caplite` | host-capability tables as data | one declaration drives checking, import order, docs, and a cross-boundary parity hash |
| `evmlite` | EVM assembler + diff-oracle interpreter | sticky build errors — a broken build never yields wrong-but-clean bytecode |
| `modlite` | wasm binary module builder | the import-after-function index shift is a structural error, not a runtime mystery |
| `litelite` | facade | `cargo add litelite` re-exports the kit |
| `prooflite` | the reference language (M1+M2) | every program halts within its fuel and provably touches only its capability table |
| `stratlite` | the strategy language (M4) | every trading decision halts within its fuel, and look-ahead is unrepresentable |
| `backtestlite` | the strategy verifier (M4) | a backtest is one reproducible integer hash; verification is compile + halt + survive + trade |

Zero external dependencies. Native + `wasm32-unknown-unknown`.

## Install

```sh
cargo add litelite      # the kernel, re-exported: diag lex parse fuel cap evm wasm
```

Every crate also stands alone — take only what you need:

```sh
cargo add fuellite                     # just the termination proof
cargo add diaglite lexlite parselite   # just the front-end kernel
cargo add prooflite                    # a total language, ready to embed
cargo add stratlite backtestlite       # strategies + their verifier
```

## The proof: `prooflite`

The smallest total language that exercises the whole kit — integers, booleans,
`let` / `if` / `repeat` / `print`, checked arithmetic, every failure a coded,
spanned diagnostic:

```rust
use prooflite::{Limits, run};

let out = run(
    "let acc = 1;
     repeat 10 { acc = acc * 2; }
     print acc;",
    Limits::default(),
)?;
assert_eq!(out.output, "1024\n");
```

The headline guarantee — **any** prooflite program halts within its fuel, and
the failure is a rendered diagnostic, not a hung process or a dead tab:

```rust
let spin = "repeat 1000000000 { }";
let err = run(spin, Limits { fuel: 1_000, output_bytes: 0 }).unwrap_err();
assert_eq!(err.code, Some(prooflite::codes::FUEL_EXHAUSTED));
println!("{}", err.render(spin));
```

```text
E0206: fuel exhausted [0..21]
line 1, col 1
  repeat 1000000000 { }
  ^^^^^^^^^^^^^^^^^^^^^
```

## Status

**0.1.0** — the kernel (M0), `prooflite` (M1), `caplite` (M2), the emitters
(M3), and the paper's core instrument (M4): `stratlite`, a strategy language
where every per-bar decision halts within its fuel and future bars are
grammatically unrepresentable (prefix-invariance is a test, not a promise),
plus `backtestlite`, whose all-integer engine makes a whole backtest one
reproducible hash and whose `verify()` is the generate→verify→keep predicate.

Pre-1.0: the APIs are honest but young. Still open: running the §5 experiment
(a model + real market data) and re-homing bashlite onto the kit (M5) — the
honest test of whether the kit carries its weight. Origin and roadmap:
[GENESIS.md](https://github.com/compusophy/litelite/blob/main/GENESIS.md).
Research plan:
[paper/OUTLINE.md](https://github.com/compusophy/litelite/blob/main/paper/OUTLINE.md).
Changes: [CHANGELOG.md](https://github.com/compusophy/litelite/blob/main/CHANGELOG.md).

This repo is constitutionally small: ≤2,000 LOC per crate, ≤25,000 total,
CI-enforced (`scripts/caps.sh`). The two predecessor projects each became
unworkable near 120K LOC; this one cannot get there.

## Build

```sh
cargo test
cargo check --target wasm32-unknown-unknown
bash scripts/caps.sh        # the caps
bash scripts/publish.sh     # rehearse a release (dry run)
```

## License

Apache-2.0
