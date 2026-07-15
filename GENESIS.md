# GENESIS — why litelite exists

Written 2026-07-14, the day the repo was created, distilling a deep-research
session in the parent project. This is the germline document: everything the
project needs to know about where it came from, so no session ever has to read
the parents into context.

## The lineage

1. **tempo-x402** (Feb–Apr 2026, 750 commits, ~120K LOC Rust): autonomous agent
   colony on Tempo — self-replicating nodes, x402 payments between agents,
   from-scratch neural nets, a compiler-verified IQ benchmark. It ended by
   pivoting to a research paper ("the Rust compiler is a free self-play
   verifier"): fine-tuning a 0.5B model on compiler-verified self-play lifted
   pass@1 from 1.5% → 16.4% on a 201-problem benchmark. The 2.3K-LOC paper
   crate outlived the 82K-LOC "soul" crate. 49% of all commits say "fix".
2. **localharness** (May 2026–, ~131K LOC): the successor product — a Rust
   agent SDK + browser-resident self-owning agents on Tempo mainnet, with
   three purpose-built language subsets inside it: **rustlite** (Rust-subset →
   wasm cartridges, 8.0K LOC/99 tests), **soliditylite** (Solidity-subset →
   EVM bytecode, 7.6K/159), **bashlite** (fuel-bounded sandboxed shell,
   3.1K/64). All zero-dep, native+wasm, live in production.
3. **litelite** (this repo): the kernel those three hand-rolled, extracted and
   paid for once — and a thesis about why that kernel is the durable layer.

## The findings that shaped the constitution

**The ~120K wall.** Both parents became unworkable near 120K LOC — context
quality, capability, and cost all degraded together. Two data points, same
developer, same model era: a hypothesis, not a law (and the wall will move as
context economics improve). But the design response is cheap insurance either
way: hard caps, small crates, and a describable surface that fits one context
window. The deeper invariant is the SURFACE cap, not the LOC cap: a repo whose
honest map no longer fits in its CLAUDE.md is already over.

**Code dies; knowledge survives.** localharness reused zero lines of
tempo-x402 — it reused the wire-format knowledge, the lessons discipline, the
cartridge concept, the verify-first instinct. The durable asset of any project
is its extractable knowledge layer (specs, lessons, postmortems, papers,
upstreamed fixes). Corollary: knowledge upstreamed into the commons (e.g. the
Tempo spec fixes, tempoxyz/tempo#6842 + tempoxyz/docs#696) is the only form
that cannot be lost. Invest in the germline deliberately.

**Harnesses melt; verifiers compound.** Every agent-harness component encodes
an assumption about what models can't do, and those assumptions go stale as
models improve (the industry knows this and says so). Verification is
different: its value is trust across an adversarial boundary, not
compensation for model weakness. A model saying "I checked my own work" is
testimony; a compiler rejecting a program is physics. As agents become
economic actors, cheap deterministic third-party-checkable verification gets
MORE valuable with model capability, not less.

**Guarantees, not languages.** A naive "small compiler" is a WEAKER verifier
than rustc. The opportunity runs the other way: smallness buys guarantees big
languages cannot give. Fuel-bounded evaluation is a termination proof (bashlite
scripts provably halt; arbitrary Rust doesn't). A host-capability table is a
complete effect bound (a rustlite cartridge provably cannot touch anything
outside its declared imports). Totality, enumerable state, decidable
properties — all become available BECAUSE the language is small. Verification
completeness scales inversely with language size. That is the product.

**Prompt discipline fails; mechanize.** Every behavioural rule that mattered in
the parents only stuck once it moved from prompt/convention into a mechanical
gate (dispatch-layer confirm guards, compile-before-publish, CI drift tests).
Hence: the constitution's caps are a CI script, the depth guard is the only
way into the parser harness, fuel is the only way to loop.

**The narrative corrodes code.** tempo-x402 put its story into its crate names
(cortex, hivemind, free-energy) and produced a 49%-fix history. The story is
fuel for humans and funding; it lives in THIS file and the paper. Code names
stay boring.

**The attractor.** A language kit is catnip for compiler-brained contributors
(and models). The leash: no milestone without a named consumer. The parents'
lesson "loop breadth, not one subsystem — rustlite=toys" is remembered here.

## What was ported at genesis (and from where)

- `diaglite` ← rustlite's `Span`/`CompileError`/`line_col`/`render_snippet`
  (already consumed verbatim by soliditylite — the existence proof of
  neutrality), generalized: `Diag` with `E{code:04}` default label.
- `lexlite` ← the byte-cursor scaffold all three parents duplicated, with the
  fixes unified: UTF-8-safe `next_char` (the mojibake bug was fixed twice,
  differently), EXPLICIT nested-vs-flat block comments (the compilers silently
  diverged), underscore-separator flag on digits (ditto).
- `parselite` ← the ~120-LOC parser harness rustlite and soliditylite carried
  as near-verbatim copies (`MAX_RECURSION_DEPTH = 96` duplicated in both;
  bashlite had NO guard — a live bug this kit structurally prevents). Plus
  `guarded()` so enter/leave pair on all paths.
- `fuellite` ← bashlite's fuel + output-cap semantics (one shared budget
  across all composition — the fractal-termination invariant) as first-class
  types.

## Roadmap

- **M0 — genesis (done).** The four kernel crates + facade, tested, wasm-green,
  caps enforced, pushed.
- **M1 — `prooflite`, the reference language (done 2026-07-15).** Smallest total language that
  exercises the whole kit: expressions, let, if, bounded loops; lex → parse →
  fueled tree-walk eval; every error a coded spanned Diag. Deliverable
  includes the kit's first external-shaped README example. Consumer: the
  paper's baseline; the kit's own proof it composes.
- **M2 — capabilities as data (done 2026-07-15).** A `CapTable` type: one declarative table per
  language → typed signatures for the checker, import emission for codegen,
  human docs, and a machine-checkable parity manifest for the far side of a
  boundary (the parents hand-sync a Rust table with a JS worker and it has
  bitten repeatedly). Consumer: prooflite's host functions; later rustlite.
- **M3 — the emitters (done 2026-07-15; the wasm one is `modlite`).** Port rustlite's wasm binary emitter and
  soliditylite's EVM assembler (`evmlite` — free on crates.io as of genesis;
  `wasmlite` is TAKEN, name the wasm one at M3) as INDEPENDENT crates
  (constitution rule 4). Consumer: M4; eventual parent re-homing.
- **M4 — `stratlite` + the experiment.** A total, fuel-bounded, backtestable
  trading-strategy language. The experiment: generate strategies with a model,
  verify mechanically (compile + halt + backtest), select survivors —
  generate→verify→keep applied to markets, with the verified-selection loop
  vs an unstructured baseline as the paper's core result. Consumer: the
  trader agent (trader.localharness.xyz).
- **M5 — re-home a parent.** bashlite moves onto the kit inside localharness,
  gaining the depth guard and spanned errors; localharness sheds LOC. The
  migration must be a net simplification there or it doesn't happen — this is
  the honest test of whether the kit carries its weight (the "seam-tax"
  question, answered by reality instead of argument).

## Post-genesis lessons

- **M1 (2026-07-15): the depth guard bounds parser recursion, not AST depth.**
  prooflite's review caught a process-killing stack overflow the tests missed:
  left-associative operator chains (`1+1+…+1`) fold ITERATIVELY in the parser
  (O(1) guard entries) yet build an AST spine the evaluator — and even the drop
  glue — later recurse down, so a flat 50K-term source aborted the process with
  fuel utterly unable to help. Fix: every fold charges one guard entry, so spine
  depth obeys the same cap as nesting; and else-if chains became a flat
  `Vec<(cond, block)>` so common flat shapes stay unbounded without deepening
  the AST. Any language on the kit must either charge iterative AST-deepening
  constructs to the guard or keep them flat. (Found by a 6-finder / 3-refuter
  adversarial review: 16 raw → 13 confirmed findings, 2 crash-grade.)

- **M2 (2026-07-15): every string a canonical artifact interpolates is an
  injection channel, and validation must BIND.** caplite's review rhymed with
  M1's: the manifest guard validated module/name but not `Ty::sym` — a comma
  in a type symbol collapsed arity (two different tables, one parity hash), a
  newline forged whole lines. And prooflite validated `host.caps()` once but
  re-fetched it at every call site, so an interior-mutability host could serve
  one table to the checker and another to dispatch — fixed by snapshotting the
  validated `Copy` table for the whole run. Validate every channel into the
  artifact; then use the validated VALUE, never a fresh fetch. (6-finder /
  3-refuter review: 16 confirmed, incl. one crash-grade-adjacent parity hole.)

- **M3 (2026-07-15): an oracle that diverges from the real machine masks
  exactly the bug class it exists to catch.** The ported EVM interpreter kept
  two of the parent's silent divergences — jump validation accepted `0x5B`
  bytes inside PUSH immediates (no JUMPDEST analysis) and reverted calls kept
  their storage writes — either of which would have green-lit a miscompiled
  contract that fails on-chain. Same shape for builders: `accept implies
  valid` means modlite must reject what engines reject (memory limits, data
  bounds), not just frame bytes prettily. And the parents' `Vec`-indexing
  panics resurfaced through a ported API that newly CLAIMED panic-freedom —
  a port inherits the old bugs but not the old excuses. (5-finder/3-refuter
  review: 21 confirmed, 3 crash-or-divergence grade.)

## The three questions only reality can answer

1. Does anyone (including the parents) actually consume the kit, or is it a
   seam-tax? (M5 answers this.)
2. Does a language built on the kit genuinely take ~a week? (M1/M4 answer
   this.)
3. Does verified-selection beat an unstructured baseline on a real task?
   (M4/the paper answer this.)

If two of the three come back negative, write the postmortem honestly and fold
the learnings back — the germline survives either way.
