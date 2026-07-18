# Where the kit sits on the decidability frontier (honest, corrected draft)

A prior draft of this note tried to present "verification completeness scales
inversely with language size" as a NEW, MEASURED relationship. An adversarial
review demolished that framing, correctly:

1. The expressiveness↔decidability tradeoff is deep, well-known **folklore** —
   Rice's theorem, Turner's *total functional programming*, the eBPF in-kernel
   verifier, Felleisen's expressiveness ordering. Formalizing it is not a
   contribution.
2. Nothing was **measured** — every cell was classified a priori by a decidability
   argument. It was a by-construction claim wearing an empirical costume.
3. The specific staircase was **wrong**: under a computability definition (the
   verifier may execute a program), a TOTAL language decides output-bound,
   termination, etc. by running-and-counting, so `prooflite` is `comp = 1.0`, not
   0.875. The fuel/byte caps are **runtime enforcement** (the `ByteBudget` clips
   and continues — it never rejects), not evidence of incomplete verification.
4. The kit's languages are **finite-state** (fixed i64/bool slots, no dynamic
   allocation), so Rice/Turing-undecidability does not even apply — their halting
   problem is decidable by cycle detection. The cost of `while` here is
   *tractability*, not *decidability*.

This corrected note keeps only what survives, states it modestly, and — the real
point — redirects the frontier search away from theory.

## What is actually true (and it strengthens the kit)

Two axes, which the prior draft wrongly fused:

**Decidability (a language-design fact).** Restricting a language to sub-Turing
constructs — finite state, bounded loops (`repeat` with an up-front count), no
recursion, **static** capability dispatch — makes the guarantees `G = {totality,
output-bound, effect-bound, determinism}` all *decidable*. The kit's languages are
built exactly this way, so they sit at the **decidability ceiling**: every `g ∈ G`
is mechanically checkable, complete, no false rejections. `prooflite` and
`stratlite` are running instances of that ceiling; general-purpose languages fall
off it (Rice). This is the standard restriction route to decidability — known, not
new — and the kit lands on it *by construction*.

**Enforcement (a resource fact, separate).** Fuel and the byte budget do NOT buy
decidability — the language is already decidable without them. They bound
*compute per program*, which is what a **training-reward oracle** needs: no rollout
can hang the loop, and each reward is cheap and deterministic. That is the honest
role of `fuellite`/`ByteBudget`, and it is a systems property, not a completeness
one. (This distinction is itself worth stating in the paper — the earlier text
blurred it.)

## The modest, real contribution

Not the tradeoff (folklore), but the **executable kit that mechanizes landing a
new language at a chosen point on the decidability frontier**: pick the guarantees,
reuse `fuellite`/`caplite`/`parselite`'s depth guard/… , and get the largest
language for which those guarantees stay complete — demonstrated by two running
instances at the ceiling plus the two compiling emitters below it. "Guarantees,
not languages," mechanized. This belongs in the paper as a **positioning +
related-work** paragraph (citing Rice, Turner, eBPF), firming up §3–§4 — *not* as a
new theorem, and not as "measured."

## Consequence for the frontier search — read this

The reason to write this down: **the theory direction is a dead end for
frontier novelty.** Done rigorously, "the thesis, measured" is a known tradeoff
dressed up; the review's verdict was "tidy, not frontier." So the frontier, if
there is one here, is **empirical**, and the ranking is:

1. **Transfer** (highest, and untested): does verifier-only self-play on a
   purpose-sized language lift a capability measured *independently* — held-out
   in-language problem-SOLVING (pass@k on specs), or cross-task? This is the actual
   tempo-x402 result (it moved a third-party benchmark); we replicated only the
   in-domain generation half. A positive transfer result is the one genuinely
   surprising, non-obvious finding available.
2. **Mechanism / baselines** (solid, the chosen Direction 2): verified self-play
   vs plain-SFT vs no-verifier. Turns "it works" into "the verifier is why."
   Cold-start (Cinit) already gives the plain-SFT point for free.
3. **The diversity-peak finding** (already in hand): the one non-obvious
   observation we have — worth deepening, but small on its own.

Recommendation: fold this corrected note into the paper as honest positioning,
drop the ladder build-out (it would formalize folklore), and spend the GPU on
**transfer** — the only lever that plausibly clears the "frontier, not tidy" bar.
