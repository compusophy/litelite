# CLAUDE.md — litelite

Read this first. It is the whole operating map; `GENESIS.md` is the origin
story + full roadmap; `paper/OUTLINE.md` is the research plan. This repo is
SELF-CONTAINED — never read the parent `localharness` repo into context (131K
LOC; the knowledge that matters was distilled here at genesis).

## What this is

A kit for **purpose-sized languages**. Thesis: smallness is not a cost — it
BUYS guarantees big languages can't give. Fuel-bounded evaluation = a
termination proof. A host-capability table = a complete effect bound. The
product is **guarantees, not languages**: pick the guarantees (halts within N
fuel, touches only these capabilities, output ≤ Y bytes), get the largest
language for which they stay mechanical.

Parents: rustlite (Rust-subset→wasm, 8.0K LOC), soliditylite
(Solidity-subset→EVM, 7.6K), bashlite (sandboxed shell, 3.1K) — all living in
`localharness`, each hand-rolled these kernel pieces with divergent bugs
(bashlite shipped with NO parser depth guard; the UTF-8 mojibake bug was fixed
twice, differently; fuel exists in three unrelated forms there). This kit pays
each invariant once.

## THE CONSTITUTION (hard rules — CI-enforced where possible)

1. **Zero external dependencies** in every kit crate. `std` only. If a feature
   needs a dep, the feature is out of scope. (Languages BUILT on the kit may
   have deps; the kit may not.)
2. **Caps** (`scripts/caps.sh`, run by CI): ≤2,000 LOC per crate, ≤25,000 LOC
   repo total, CLAUDE.md ≤8,000 chars. At a cap: split, shrink, or kill —
   never raise the cap. The parents both died at ~120K LOC; the cap is why
   this repo can't.
3. **No milestone without a named consumer.** Nothing lands "because it's
   cleaner." Every change names who needs it (a language in this repo, a
   paper experiment, or an external consumer like localharness/bashlite).
   This repo is a compiler-nerd attractor; the consumer rule is the leash.
4. **Never a unified codegen trait.** wasm (structured/relative control flow),
   EVM (absolute jumps/labels), and tree-walk evaluation are semantically
   irreconcilable — proven in the parents. Emitters ship as independent
   libraries, period.
5. **wasm32 always green**: `cargo check --target wasm32-unknown-unknown`
   passes at every commit. No cfg-gated escape hatches in kit crates.
6. **Narrative lives in GENESIS.md and the paper, never in code.** Crate and
   symbol names stay boring and literal. (The predecessor project named nine
   crates after a consciousness metaphor; 49% of its commits say "fix".)
7. **Naming: NO dashes, lite goes LAST.** Crates are single dashless words
   with the `lite` suffix (`diaglite`, `lexlite`, `parselite`, `fuellite`;
   languages too: `bashlite`, `prooflite`, `stratlite`). The facade alone is
   `litelite` — the kit of lites. Never write the dashed form anywhere
   (caps.sh greps for it).
8. **Don't silently miscompile/misparse** — every failure is a coded, spanned
   `Diag`. A wrong-but-clean result is worse than an error.
9. Commit AND push promptly; unpushed work is invisible.

## Map

```
Cargo.toml            workspace + the `litelite` facade crate (src/lib.rs re-exports)
crates/
├── diaglite/   Span, Diag (message+span+u16 code), line_col, render_snippet
│               (caret snippets; floor_char_boundary guards mid-char offsets)
├── lexlite/    Cursor byte-lexer kit: eat/eat_while/spans, line+block comments
│               (nested flag EXPLICIT), eat_ident/eat_decimal/eat_hex
│               (underscore flag), next_char (UTF-8-safe — never byte-cast)
├── parselite/  TokCursor<T: Tok> recursive-descent harness: clamping advance
│               (EOF-sentinel convention), eat(pred), enter/leave depth guard
│               (DEFAULT_MAX_DEPTH=96 — wasm-stack abort rationale in the doc
│               comment), guarded() pairs enter/leave on all paths
├── fuellite/   Fuel (burn/Exhausted — pass ONE &mut Fuel into every
│               sub-evaluation; never fork a child budget) + ByteBudget
│               (grant/push_str clip-at-char-boundary/push_bytes)
├── caplite/    M2: Cap/CapTable as DATA. trait Ty (syms are ABI), check_args,
│               validate/validate_flat (ident-only strings — EVERY interpolated
│               string is an injection channel), docs_markdown, versioned
│               parity manifest + FNV-1a-64 hash for the far side of a boundary
├── evmlite/    M3 emitter: asm (op SSOT, minimal-width push, two-pass PUSH2
│               label back-patch, init_wrapper; STICKY AsmError — never panic
│               or truncate) + interp, its diff-oracle (step/mem/stack caps,
│               JUMPDEST analysis excludes PUSH immediates, revert rolls back
│               storage, KECCAK256=Unsupported: no hashing dep)
├── modlite/    M3 emitter: wasm module builder (wasmlite was taken) — LEB128,
│               functype interning, locals RLE, section framing; sticky
│               BuildError makes import-after-func index shift + spec-invalid
│               limits structural errors. Bodies are consumer bytes.
├── prooflite/  M1+M2 reference language ON the kit (not in the facade):
│               total, fuel-bounded; i64+bool, let/assign/print/if/repeat,
│               checked arithmetic, host calls via caplite (Host trait,
│               run_with_host — table SNAPSHOTTED once per run); codes lex
│               E00xx/parse E01xx/eval-host E02xx. NB: binary folds charge the
│               depth guard — it bounds parser recursion, NOT AST depth
scripts/caps.sh       the constitution's teeth (LOC + CLAUDE.md caps)
paper/OUTLINE.md      the paper IS the product; experiments land as sections
GENESIS.md            origin, distilled parent learnings, roadmap M0–M5

```

(The port/ snapshot staging area is gone: all five parent snapshots were
consumed and deleted as their ports landed, per its own rule.)

## Build / test

```sh
cargo test                                          # workspace + facade
cargo check --target wasm32-unknown-unknown         # must stay green
cargo fmt --check && cargo clippy -- -D warnings
bash scripts/caps.sh                                # the caps
```

## Roadmap (detail in GENESIS.md)

- **M0 (done at genesis):** kernel crates ported from the parents, tested.
- **M1 (done):** `prooflite` — total reference language on the kit (lex→parse→
  fueled eval) + README example. Consumer: the paper's baseline.
- **M2 (done):** `caplite` — capability tables as data (typed sigs, import
  order, docs, parity manifest+hash). Consumer: prooflite hosts; later rustlite.
- **M3 (done):** the emitters as independent crates: `evmlite` (asm + oracle)
  and `modlite` (wasm module builder; wasmlite was taken). Consumer: M4 +
  eventual parent re-homing.
- **M4:** `stratlite` — total, fuel-bounded, backtestable trading-strategy
  language. Consumer: the trader agent; generate→verify→keep selection loop =
  the paper's core experiment.
- **M5:** re-home bashlite onto the kit inside localharness (it gains the
  depth guard + spanned errors). Consumer: localharness, −LOC there.

## Context / lineage (one paragraph, so you never need the parent repo)

localharness (github.com/compusophy/localharness) is the product this grew out
of: browser-resident self-owning agents on Tempo (payments L1). Its author
found the Tempo tx-0x76 spec bugs (fee-payer hash preimage) — upstreamed as
tempoxyz/tempo#6842 + tempoxyz/docs#696. Its predecessor tempo-x402 (agent
colony, 750 commits) ended as a paper: compiler-verified self-play lifted a
0.5B model from 1.5%→16.4% pass@1 — the empirical seed of this repo's thesis
that cheap mechanical verification is the durable layer of the agent stack.
A possible sibling extraction (separate repo, not here): `tempotx`, the ~2.4K
LOC Tempo tx encoder from localharness.
