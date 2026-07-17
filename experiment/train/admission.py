"""Build the SFT admission set from verifier rewards.

This is the part of the M6 trainer that encodes the red-team's anti-collapse
guards, and it is deliberately torch-free so it can be tested without a GPU.
The training loop (train.py) shells out to `s5 reward` and calls this to decide
what to fine-tune on.

Every rule here answers a specific reward hack the judge panel named:

  * Ok-rung only. The 2/3 gate rung is a farmable local optimum -- `lookback 4;`
    or a comment-only body compiles, runs clean, gate-fails, and collects 2/3
    for the absence of a strategy. Admitting gate-rung samples would train the
    model toward the shortest thing that merely reaches the gate. So only
    class == "ok" (a valid, active strategy) is admitted.
  * Dedup by nkey, capped per key. `s5 reward` emits a source-canonical dedup
    key that works at every rung (equity_hash is 0 below survivor and gameable
    above it). Capping any one key's share stops the model collapsing onto a
    single template and emitting re-commented copies of it.
  * Per-style cap. Aggregate survivor rate can rise by collapsing to the single
    easiest prompt style. Capping each style's share keeps the admission set
    spread across the intended behaviors, not just the easy one.

The reward VALUE is not consulted here -- admission is by class and diversity,
never by pnl or by the scalar. That keeps the held-out generalization result
non-circular: we never train on the quantity we then measure.
"""

from __future__ import annotations

import json
from collections import Counter
from dataclasses import dataclass, field


@dataclass
class Rollout:
    """One sampled program and the prompt/style that produced it."""

    id: str
    style: str
    source: str


@dataclass
class Reward:
    """One reward record as `s5 reward` emits it (the fields we use)."""

    id: str
    cls: str
    nkey: str
    fuel: int


@dataclass
class AdmissionStats:
    """What the round learned, for the histogram-as-learning-curve log."""

    histogram: Counter = field(default_factory=Counter)  # class -> count
    distinct_nkeys: int = 0
    per_style_admitted: Counter = field(default_factory=Counter)
    fuel_spectrum: list[int] = field(default_factory=list)  # admitted, sorted


def extract_source(completion: str) -> str:
    """Pull a stratlite program out of a raw model completion. A local sampler
    has no constrained decoding, so it emits prose and code fences; take the
    first ```-fenced block if present, else the raw text. This NEVER repairs a
    program -- it only unwraps -- because the reward oracle is strict on purpose
    (fenced junk earns a compile-zero, the correct training signal). Testable
    without a GPU, which is why it lives here and not in train.py.
    """
    if "```" in completion:
        parts = completion.split("```")
        if len(parts) >= 3:
            body = parts[1]
            # drop an optional language tag on the fence's opening line
            return body.split("\n", 1)[1] if "\n" in body else body
    return completion


def parse_rewards(jsonl: str) -> dict[str, Reward]:
    """Parse `s5 reward` output, keyed by id (results may arrive in any order)."""
    out: dict[str, Reward] = {}
    for line in jsonl.splitlines():
        line = line.strip()
        if not line:
            continue
        r = json.loads(line)
        out[r["id"]] = Reward(id=r["id"], cls=r["class"], nkey=r["nkey"], fuel=int(r["fuel"]))
    return out


def build_admission_set(
    rollouts: list[Rollout],
    rewards: dict[str, Reward],
    *,
    per_key_cap: int = 2,
    per_style_frac: float = 0.5,
) -> tuple[list[Rollout], AdmissionStats]:
    """Return the rollouts to SFT on, plus the round's diagnostics.

    A rollout is admitted iff it is class "ok" AND its source-canonical key has
    not already filled its per-key quota AND its style has not exceeded its
    share of the set. Order is preserved so a run is reproducible given the same
    inputs.
    """
    stats = AdmissionStats()
    for ro in rollouts:
        rw = rewards.get(ro.id)
        if rw is not None:
            stats.histogram[rw.cls] += 1

    # The per-style cap is a fraction of the total ADMITTED set, which we do not
    # know until we finish -- so cap against the count of ok-rung candidates,
    # the largest the set can be. This is a conservative, deterministic bound.
    ok_total = sum(1 for ro in rollouts if (r := rewards.get(ro.id)) and r.cls == "ok")
    style_cap = max(1, int(ok_total * per_style_frac))

    admitted: list[Rollout] = []
    per_key: Counter = Counter()
    per_style: Counter = Counter()
    seen_keys: set[str] = set()
    for ro in rollouts:
        rw = rewards.get(ro.id)
        if rw is None or rw.cls != "ok":
            continue
        seen_keys.add(rw.nkey)
        if per_key[rw.nkey] >= per_key_cap:
            continue
        if per_style[ro.style] >= style_cap:
            continue
        per_key[rw.nkey] += 1
        per_style[ro.style] += 1
        admitted.append(ro)
        stats.fuel_spectrum.append(rw.fuel)

    stats.distinct_nkeys = len(seen_keys)
    stats.per_style_admitted = per_style
    stats.fuel_spectrum.sort()
    return admitted, stats
