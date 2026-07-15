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
2. **The kit** — diaglite/lexlite/parselite/fuellite; invariants paid once;
   the three-parent dedup evidence (missing depth guard, twice-fixed mojibake,
   three fuels).
3. **prooflite** — the reference total language; what "every program halts within
   its fuel" costs in expressiveness, measured.
4. **Experiment: language-construction cost** — wall-clock + LOC + defect
   count building the Nth lite language on the kit vs the parents' hand-rolled
   baselines (rustlite 8.0K/99t, soliditylite 7.6K/159t, bashlite 3.1K/64t).
5. **Experiment: verified selection (stratlite)** — generate trading
   strategies with a model; verify (compile + halt + backtest); select.
   Compare against unstructured generation on held-out data. The
   generate→verify→keep loop with real selection pressure.
6. **Limits** — what smallness cannot buy (semantic correctness beyond the
   checked properties); Goodhart risks (caps pushing complexity into seams);
   when a general-purpose language + tests beats a purpose-sized language.

## Rules

- Every number in the paper is reproducible by a command in this repo.
- Negative results get their section; "honest reproducibility" is the house
  style inherited from the predecessor paper.
