# Paper outline — "Purpose-sized languages: buying total verification with smallness"

> The paper IS the product (GENESIS.md). Every experiment lands here as a
> section with its numbers and repro commands. Predecessor result (tempo-x402,
> 2026-04): compiler-verified self-play lifted a 0.5B model 1.5%→16.4% pass@1 —
> one verifier (rustc), one task family. This paper generalizes the verifier
> side.

## Claim

For agent-generated programs, verification completeness scales inversely with
language size. A purpose-sized language can make properties MECHANICAL that
are undecidable or unchecked in general-purpose languages — termination (fuel),
effect containment (capability tables), output bounds (byte budgets) — and
those mechanical guarantees are exactly what generate→verify→keep loops and
inter-agent commerce need.

## Sections (planned)

1. **Motivation** — verifiers as the durable layer of the agent stack; the
   trust-boundary argument (testimony vs physics).
2. **The kit** — diaglite/lexlite/parselite/fuellite/caplite plus the two
   independent emitters (evmlite, modlite); invariants paid once; the
   three-parent dedup evidence (missing depth guard, twice-fixed mojibake,
   three fuels, the triple-declared rustlite host table that caplite kills as
   a class: one table → sigs, import order, docs, parity hash). M3's port
   lesson (an oracle diverging from the real machine masks the bug class it
   exists to catch — GENESIS post-genesis lessons) belongs to the limits
   discussion in §6 too.
3. **prooflite** — the reference total language; what "every program halts within
   its fuel" costs in expressiveness, measured. Landed 2026-07-15 (M1):
   1,545 LOC incl. tests (`bash scripts/caps.sh`), 28 tests + 1 doctest
   (`cargo test -p prooflite`), one session on the kit. M2 (same day) added
   the complete effect bound — host capabilities as a caplite table (checked
   calls, per-cap fuel costs, parity manifest) — at 1,997/2,000 LOC: the
   constitutional cap fired twice and was answered by shrinking both times.
4. **Experiment: language-construction cost** — wall-clock + LOC + defect
   count building the Nth lite language on the kit vs the parents' hand-rolled
   baselines (rustlite 8.0K/99t, soliditylite 7.6K/159t, bashlite 3.1K/64t).
   First data point (prooflite, N=1): one session, 1,545 LOC, and a 13-finding
   review whose two crash-grade defects were BOTH instances of one lesson —
   the parser depth guard bounds recursion, not AST depth (GENESIS,
   post-genesis lessons) — now paid once in the kit's consumption idiom.
5. **Experiment: verified selection (stratlite)** — generate trading
   strategies with a model; verify (compile + halt + backtest); select.
   Compare against unstructured generation on held-out data. The
   generate→verify→keep loop with real selection pressure.
   INSTRUMENT LANDED 2026-07-15 (M4): `stratlite` (1,910 LOC incl tests) +
   `backtestlite` (655) — `verify()` returns the Reject{Compile,Run,Gate}
   histogram, `Report::equity_hash` makes every backtest one reproducible
   number, `stratlite::REFERENCE` is the generation prompt's language card
   (a const, so prompt/verifier drift is impossible), and no-look-ahead is
   pinned by the prefix-invariance test. STILL NEEDED to run §5: a model
   (generation), real candle data with a held-out split, and the thin
   harness gluing them — all outside the kit by design (the harness may use
   f64 and deps). Repro once run: every number traces to
   `cargo test -p stratlite -p backtestlite` + the harness's pinned seeds.
6. **Limits** — what smallness cannot buy (semantic correctness beyond the
   checked properties); Goodhart risks (caps pushing complexity into seams);
   when a general-purpose language + tests beats a purpose-sized language.

## Rules

- Every number in the paper is reproducible by a command in this repo.
- Negative results get their section; "honest reproducibility" is the house
  style inherited from the predecessor paper.
