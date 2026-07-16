# CLAUDE.md — litelite

Read this first. It is the whole operating map; `GENESIS.md` is the origin
story + full roadmap; `paper/OUTLINE.md` is the research plan. This repo is
SELF-CONTAINED — never read the parent `localharness` repo into context; its
knowledge was distilled here at genesis.

## What this is

A kit for **purpose-sized languages**. Thesis: smallness is not a cost — it
BUYS guarantees big languages can't give. Fuel-bounded evaluation = a
termination proof. A host-capability table = a complete effect bound. The
product is **guarantees, not languages**: pick the guarantees (halts within N
fuel, touches only these capabilities, output ≤ Y bytes), get the largest
language for which they stay mechanical.

Parents: rustlite, soliditylite, bashlite (~19K LOC in `localharness`), each
hand-rolled these kernel pieces with divergent bugs (a missing depth guard,
the twice-fixed mojibake bug, three unrelated fuels — GENESIS has the full
story). This kit pays each invariant once.

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
   symbol names stay boring and literal.
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
├── evmlite/    M3 emitter: asm (op SSOT, min-width push, two-pass PUSH2 label
│               back-patch, init_wrapper; sticky AsmError — never panic or
│               truncate) + interp diff-oracle (step/mem/stack caps, real
│               JUMPDEST analysis, revert rollback, keccak=Unsupported)
├── modlite/    M3 emitter: wasm module builder (wasmlite taken) — LEB128,
│               functype interning, locals RLE, section framing; sticky
│               BuildError (import-after-func shift, spec-invalid limits)
├── prooflite/  M1+M2 reference language ON the kit (not in the facade):
│               total, fuel-bounded; i64+bool, let/assign/print/if/repeat,
│               checked arithmetic, host calls via caplite (Host trait,
│               run_with_host — table SNAPSHOTTED once per run); codes lex
│               E00xx/parse E01xx/eval-host E02xx. NB: binary folds charge the
│               depth guard — it bounds parser recursion, NOT AST depth
├── stratlite/  M4 language: total, fuel-bounded strategies. lookback pragma,
│               var slots (bar-ATOMIC: faulted bars roll back), per-bar body →
│               signal; fresh fuel/bar; indicator windows are static literals;
│               no look-ahead BY CONSTRUCTION (prefix-invariance tested);
│               REFERENCE = the prompt card, a const of the crate
├── backtestlite/ M4 verifier: deterministic integer engine (fills at next
│               open, adverse validated Costs), Report: Eq + equity_hash
│               (FNV over the final curve), Gate, verify() →
│               Reject{Compile,Run,Gate} — paper §5's predicate. Codes E03xx
scripts/caps.sh       the constitution's teeth (LOC + CLAUDE.md caps)
scripts/publish.sh    dry run by default, --execute uploads; resumable (crates.io
                      rate-limits NEW crates: a first publish can stop partway)
CHANGELOG.md          ONE version across every crate; a tag's notes come from here
paper/OUTLINE.md      the paper IS the product; experiments land as sections
GENESIS.md            origin, distilled parent learnings, roadmap M0–M5
experiment/           §5's RUN: model + pinned candles + harness. NOT a workspace
                      member — it takes deps; rules 1+5 keep them out of the kit

```

## Build / test / release

```sh
cargo test                                          # workspace + facade
cargo check --target wasm32-unknown-unknown         # must stay green
cargo fmt --check && cargo clippy -- -D warnings
bash scripts/caps.sh                                # the caps
bash scripts/publish.sh                             # rehearse a release
```

Version lives ONCE in `[workspace.package]` (cargo rejects drift against
`[workspace.dependencies]`). Tag `vX.Y.Z` → `release.yml`: every gate,
tag==version, CHANGELOG has that section, publish, GitHub release. Needs
`CARGO_REGISTRY_TOKEN`.

## Roadmap (each milestone's consumer + lesson: GENESIS.md)

- **M0–M4 (done, live on crates.io at 0.1.0):** the Map above IS the result.
- **M5:** re-home bashlite onto the kit inside localharness (it gains the
  depth guard + spanned errors). Consumer: localharness, −LOC there — and the
  honest test of whether the kit carries its weight.
- **§5 RUN (open):** `experiment/` tests verified SELECTION; language size is
  §4's claim (construction cost vs the parents).

## Context / lineage (GENESIS.md has the full story)

localharness (github.com/compusophy/localharness): browser-resident
self-owning agents on Tempo. Its predecessor tempo-x402 ended as a paper —
compiler-verified self-play lifted a 0.5B model 1.5%→16.4% pass@1 — the
empirical seed of this repo's thesis.
