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

## State of the evidence (updated 2026-07-19 — the handoff block)

Any session can resume paper work from THIS file plus the cited artifacts; no
conversation context is required. Heavy drafting runs as multi-agent workflows
(section writers + a hostile referee), workers on Opus-class models.

- **§1–§3: fully evidenced.** GENESIS.md (lineage, dedup findings, the two
  crash-grade prooflite lessons); numbers from `bash scripts/caps.sh` and
  `cargo test -p prooflite`.
- **§4: data exists in git history.** One-session-per-language claims trace to
  commit timestamps (`git log`); parents' baselines are in GENESIS. Caveats to
  keep: same developer + model era, N=2, LOC is a proxy.
- **§5: instrument landed; the FINE-TUNE RAN; one arm still pending.** The
  key-free corpus run is committed (`experiment/corpus/README.md`). The M6 GPU
  fine-tune has been RUN on the 3090 and its result committed
  (`experiment/results/README.md`): verifier-only fine-tuning took Qwen3-0.6B
  from a measured 0.0% valid-program rate to 100% compile / ~96% held-out
  gate-clear across a train month, a 5-month-distant month, and a cross-asset
  window — reproduce the SCORING via `cd experiment && ./target/release/s5 eval
  results/{base,c7}.jsonl data/<window>.csv` (the model is not reproducible; the
  pools + candles are committed). Design in `experiment/M6.md`. The N=2 replication on
  prooflite is committed (`experiment/proofbench/results/README.md`, paper
  §5.7): 3.5% → ~95% RICH, ~100% corpus-novel. The TRANSFER benchmark ran
  2026-07-19 (paper §5.8, same README): 30 held-out specs, exact-output
  pass@8 — base 20% / Cinit (plain SFT) 80% / C6 (self-play) 43.3%. Transfer
  is real, AND self-play overwrites spec-following with the reward's output
  shape (8 of C6's 17 misses compute the right answer then pad; the loss
  localizes to the problems whose correct output is short, where the RICH
  rung disprefers the correct shape). The FIX ARM ran the same day
  (2026-07-19): spec-conditioned self-play (48 training specs, reward top
  rung = exact output match, `p6 solvereward`) from the same Cinit → S5
  93.3% pass@8 (hard tier 10/12 with training families disjoint from it),
  while unconditional RICH drops to 24.2% — each arm maximizes its own
  reward and degrades the other axis; the symmetry is the §5.8 finding.
  Repro: `cd experiment/proofbench &&
  ./target/release/p6 solve problems/heldout.jsonl results/solve_s5.jsonl`.
  N=3 RAN 2026-07-19 (§5.8's close): applite + a8's BEHAVIORAL reward
  (event scripts), 0/16 → 16/16 held-out behavioral pass@8 in ~1h — the
  loop as a product (`app/` shell + local generator), not a benchmark.
  STILL PENDING:
  the frozen-big-model A/B arm — there is NO Anthropic API key and there will
  not be one; that arm stays a protocol slot unless a keyless generator fills it.
- **§6: real findings on hand.** Fuel bound free on this task (max 186 of
  25,000 = 0.74%); the single-month benchmark has no out-of-sample teeth
  (conditional gate-clear gap 1.5 points — `s5 eval`); reward hacks found in
  our own reward and closed (empty rollout, gate-rung farming, dedup-key
  blindness). The seam-tax question stays open until M5.

## Rules

- Every number in the paper is reproducible by a command in this repo.
- Negative results get their section; "honest reproducibility" is the house
  style inherited from the predecessor paper.
