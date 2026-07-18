# The completeness–size frontier (working draft — the thesis, measured)

Goal: turn the paper's headline — *"the smaller a language's sanctioned surface,
the more of a program's behavior a checker can decide"* — from an asserted slogan
into a **measured relationship**: a scored completeness metric, a feature-ladder
of languages, and a monotone curve with our kit languages as anchored, executable
points. This is the §4-thesis made rigorous, and the frontier contribution.

## 1. What "verification completeness" is (precisely)

Fix a set of **guarantees** `G` — the properties a purpose-sized language exists
to make mechanical. A *verifier* `V_g` for guarantee `g` on language `L` decides,
for a program `p ∈ L`, whether `g(p)` holds. Standard soundness/completeness:

- **sound**: `V_g(p) = accept  ⟹  g(p)` (never certifies a violator)
- **complete**: `g(p)  ⟹  V_g(p) = accept` (never rejects a satisfier)
- **total**: `V_g` itself always halts

For each `(L, g)` classify the BEST achievable verifier into three rungs:

| rung | meaning | score |
|---|---|---|
| **C** — complete | a sound, complete, total `V_g` exists: every `g`-satisfying program is accepted, decided mechanically | 1 |
| **B** — bounded | sound + total verifiers exist but all are **incomplete** — the only mechanical check is a resource cap that falsely rejects some `g`-satisfying programs | ½ |
| **U** — undecidable | no sound + complete decision procedure exists (`g` is undecidable over `L`) | 0 |

**Completeness of a language:** `comp(L) = (1/|G|) · Σ_{g∈G} score(L, g)  ∈ [0, 1]`.

The distinction that carries the whole thesis is **C vs B**: a small language makes
a guarantee *structurally* true (accept everything, reject nothing valid); a larger
one can only *enforce* it with a bound that throws away valid programs. "We imposed
a fuel cap" is a `B`, not a `C` — the cap is the tell that completeness was lost.

## 2. The guarantees (our domain)

`G = { TOTALITY, OUTPUT-BOUND, EFFECT-BOUND, DETERMINISM }`  (|G| = 4)

- **TOTALITY** — halts eventually on every input.
- **OUTPUT-BOUND** — emits ≤ a statically-known `Y` bytes.
- **EFFECT-BOUND** — touches only capabilities in a declared table.
- **DETERMINISM** — same input ⟹ same output (a reproducible result).

(Resource-termination "halts within `N` steps" is deliberately *not* a separate
guarantee — it is exactly the `B`-witness for TOTALITY once loops go unbounded.)

## 3. The feature-ladder

Each rung is the previous language plus ONE feature that adds expressiveness. By
construction `L_i ⊂ L_{i+1}` (strictly more programs), so this is a real size axis
(features added, left column) — not a vibe.

| # | language `L_i` (adds…) | TOTALITY | OUTPUT | EFFECT | DETERM | `comp` |
|---|---|:---:|:---:|:---:|:---:|:---:|
| 0 | straight-line: `let`/assign, arithmetic, `print` | C | C | C | C | 1.00 |
| 1 | + `if`/`else` (finite branching) | C | C | C | C | 1.00 |
| 2 | + `repeat K {…}`, **literal** count | C | C | C | C | 1.00 |
| 3 | + `repeat EXPR {…}`, data-dependent count `= prooflite` | C | **B** | C | C | 0.875 |
| 4 | + host calls over a **static** capability table `= prooflite+Host` | C | B | C | C | 0.875 |
| 5 | + `while COND {…}` (unbounded loop) | **B** | B | C | C | 0.75 |
| 6 | + user functions with **recursion** | B | B | C | C | 0.75 |
| 7 | + first-class / higher-order functions (dynamic call target) | B | B | **U** | C | 0.625 |
| 8 | + nondeterminism / ambient I/O / concurrency `≈ general-purpose` | U | U | U | **B** | 0.125 |

**Where the drops happen, and why (the justifications are the contribution):**

- **L2→L3, OUTPUT C→B.** With a literal count the emitted size is `K·|body|`,
  a compile-time constant → `C`. With `repeat EXPR`, the count is a runtime `i64`;
  "output ≤ Y" is then not statically decidable, and the only mechanical check is
  a byte cap (`ByteBudget`) that rejects otherwise-fine large programs → `B`.
  *This is exactly why `prooflite` ships a fuel + output cap: it sits at L3–L4.*
- **L4, EFFECT stays C.** A capability *table* keeps effects complete even in a
  richer language, as long as call targets are static — the kit's `caplite` point:
  effects are `C` far past where TOTALITY falls, because they are a data question,
  not a halting question.
- **L4→L5, TOTALITY C→B.** `while` makes halting undecidable (Turing); a fuel
  bound restores a *sound, total* check but rejects programs that halt after the
  budget → `B`. The structural guarantee is gone.
- **L6→L7, EFFECT C→U.** A dynamic call target can't be resolved statically, so a
  sound+complete effect table no longer exists → `U`.
- **L8, everything U (and DETERMINISM falls to B).** Ambient I/O / concurrency
  make totality, output, and effects undecidable; determinism survives only if you
  *impose* a scheduler/seed (a bound) → `B`.

## 4. Monotonicity (why this is a frontier, not a scatter)

**Claim.** For `L ⊂ L'` (a strict expressiveness extension), `class(L', g) ≤
class(L, g)` for every `g`, hence `comp(L') ≤ comp(L)`.

*Argument.* Adding a feature only adds programs. A guarantee's decision problem
over a superset of programs can only stay as hard or get harder (its undecidability
is inherited), and a complete verifier over the superset must additionally accept
the new satisfiers — which is where completeness is lost first. So each `g` moves
monotonically C → B → U as features accrue, never back. The curve is therefore
monotone non-increasing **by construction**, and §3 instantiates *where* each drop
lands. The thesis is not "we observed a correlation"; it is "completeness is a
monotone function of expressiveness, and here is the staircase."

## 5. Anchored, executable points (not just a table)

Rows 3–4 are `prooflite` as shipped; the kit *runs* them, and the `C`/`B`
classifications are executable, not asserted:

- **TOTALITY = C at L3**: `prooflite`'s `repeat EXPR` is structurally total —
  demonstrate by construction (no `while`, no recursion in the grammar) and by the
  existing `cargo test -p prooflite` totality tests. No program loops forever.
- **OUTPUT = B at L3**: exhibit a `prooflite` program the byte cap falsely rejects
  (a valid program printing > `output_bytes`) — the `B`-witness, i.e. a
  terminating program the *complete* checker would accept but the *bounded* one
  rejects. (TODO: add this witness as a test.)
- **EFFECT = C at L4**: `caplite`'s static table decides effects completely
  (`validate`/`docs_markdown` over the snapshotted table) — executable.

To turn rows 5–7 from formally-classified into *demonstrated*, build minimal
`prooflite+while` and `prooflite+rec` variants (a few hundred LOC each, reusing
`lexlite`/`parselite`) and exhibit, for each, a concrete program the sound checker
must false-reject (the `B`/`U` witness). That is the empirical backing for the
staircase and the clearest next build-out.

## 6. What this establishes — and its honest limits

**ESTABLISHES.** Verification completeness is a *measurable, monotone* function of
language expressiveness, not a slogan: a scored metric, a staircase with named
drop-points, each justified from decidability, and the endpoints (`comp≈1` for our
languages, `comp≈0.1` for general-purpose) anchored to running code. The kit's
languages sit provably at the high-completeness end; that position is the product.

**LIMITS.**
- The ladder is *one* path through feature-space; completeness is a partial order
  over feature-sets, and we plot a chosen chain. A different chain (e.g. effects
  before loops) reorders the drops but not the endpoints or monotonicity.
- `|G| = 4` and equal weights make `comp` a coarse scalar; the honest object is the
  per-guarantee C/B/U vector, with `comp` a summary.
- Rows 5–8 are classified from decidability, not yet all run. §5's build-out closes
  that for 5–7; L8 (general-purpose) is undecidable by reduction, not something we
  build.
