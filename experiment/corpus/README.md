# experiment/corpus — a verified stratlite corpus

`seed.jsonl` is 134 stratlite programs (`{id, style, source}`), generated with
NO API key: six agents, one per strategy family (trend, mean-reversion,
breakout, momentum, stateful, combo), each given `stratlite::REFERENCE` and
asked for diverse valid programs. This is generate→verify→keep with agents as
the generator instead of a frozen API — the §5 pool and the M6 cold-start SFT
set, both key-free.

Every number below reproduces from a command in this repo (the generation does
not — that is the honest split; the corpus is committed rather than
regenerated).

## Verified against the real engine

`s5 reward experiment/corpus/seed.jsonl <candles.csv>` on BTCUSDT-1h-2024-01:

- 134 programs, **100% compile, 99% survivor rate** (132 ok, 2 gate, 0 run, 0
  compile-fail). Agents plus the language card produce valid, active stratlite
  almost perfectly.

## The fuel bound is free on this task — measured, not assumed

Across the 132 survivors, `max_fuel_per_bar` is **min 16 / median 55 / max 186**
against a 25,000/bar cap — the worst diverse strategy uses **0.74% of the
budget**. So on this task the termination guarantee never came close to
binding: it is free and, as a discriminator, untested. This is §6's honest
sentence, and it is a distribution, not the binary E0206 count that cannot tell
"the bound did work" from "the bound was decoration."

## The single-month benchmark has no out-of-sample teeth — flagged, not hidden

`s5 eval experiment/corpus/seed.jsonl <candles.csv>` reports the CONDITIONAL
metric: among compilers, gate-clear is 98.5% on train and 97.0% on held-out —
a **1.5-point gap, near zero**. On one month, held-out is no harder than train,
so raw survivor rate would be a meaningless generalization signal here (only
mean-reversion shows any train/held-out spread). This is exactly the red-team's
warning (`../M6.md`), confirmed empirically: the M6 generalization experiment
needs a wider regime split — a chronologically distant test window and a second
asset — before any "lift" on this benchmark means anything.
