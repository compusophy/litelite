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
| `caplite` | host-capability tables as data | one declaration drives checking, import order, docs, and a cross-boundary parity hash |
| `evmlite` | EVM assembler + diff-oracle interpreter | sticky build errors — a broken build never yields wrong-but-clean bytecode |
| `modlite` | wasm binary module builder | the import-after-function index shift is a structural error, not a runtime mystery |
| `litelite` | facade | `cargo add litelite` re-exports the kit |
| `prooflite` | the reference language (M1+M2) | every program halts within its fuel and provably touches only its capability table |

Zero external dependencies. Native + `wasm32-unknown-unknown`.

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

M3. The kernel (M0), `prooflite` (M1), `caplite` (M2), and the two independent
emitters (M3): `evmlite` — an EVM assembler whose diff-oracle interpreter
deploys and calls assembled bytecode in-process with chain-faithful semantics
(JUMPDEST analysis, revert rollback, hard step/memory/stack bounds) — and
`modlite`, a wasm module builder that turns the binary format's ordering and
validity rules into structural errors. Next: `stratlite` and the paper's core
experiment (M4). Roadmap and origin: [`GENESIS.md`](GENESIS.md). Research
plan: [`paper/OUTLINE.md`](paper/OUTLINE.md).

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
