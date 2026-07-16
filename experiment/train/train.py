"""M6 rejection-sampling SFT loop (expert iteration) -- the tempo-x402 recipe.

    ┌─ sample K programs per style from the current policy   (torch)
    │  write rollouts.jsonl {id, style, source}
    ├─ score them with the verifier                          `s5 reward`
    ├─ build the SFT admission set                           select.py (tested)
    │  keep Ok-rung only, dedup by nkey, cap per style
    ├─ SFT the policy on the admitted {prompt, source}       (torch)
    └─ checkpoint, log the round's Reject histogram + fuel spectrum
       repeat until the held-out gate-clear lift plateaus

NOT RUNNABLE IN THIS REPO'S CI: it needs a GPU and torch/transformers. The
load-bearing part -- the admission-set guards in select.py -- IS tested here
with plain python, because that is where the reward hacks live. The torch calls
below are deliberately isolated behind `Policy` so the orchestration is
readable and the ML dependency is a thin shell, not the whole file.

Reproducibility, honestly: the model is NOT reproducible (no seed makes a
fine-tune deterministic across hardware, and sampling is stochastic). What
reproduces from a command is the REWARD -- `s5 reward` is deterministic -- and
therefore, given a pinned checkpoint, its eval numbers. So the run COMMITS its
checkpoints and rollouts as artifacts; it does not pretend to regenerate them.
"""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass

from select import Rollout, build_admission_set, parse_rewards

# The generation card and the eight styles are stratlite's OWN const and the
# §5 STYLES list -- kept in ONE place so the trainer, the §5 harness, and the
# verifier can never disagree about the language. In practice the trainer reads
# them from the s5 binary rather than copying them here; this list is the shape.
STYLES = [
    "a trend-following strategy using a fast/slow sma crossover",
    "a mean-reversion strategy using rsi extremes",
    "a breakout strategy using highest() and lowest() channels",
    "a momentum strategy using ema and a position() check",
    "a strategy that uses var state to avoid flipping position every bar",
    "a strategy that goes flat when the recent range is narrow",
    "a strategy combining an rsi filter with an sma trend check",
    "a conservative strategy that trades rarely",
]


@dataclass
class Config:
    base_model: str = "Qwen/Qwen2.5-Coder-0.5B"  # small: the trader runs it on-device
    s5_bin: str = "../target/release/s5"  # the tested reward oracle
    candles: tuple[str, ...] = ()  # pinned klines CSVs (train+val windows)
    samples_per_style: int = 128  # K
    rounds: int = 12
    per_key_cap: int = 2
    per_style_frac: float = 0.5


class Policy:
    """The ONLY torch-dependent surface. Everything else is plain python so the
    guards stay testable without a GPU. Left unimplemented on purpose -- filling
    these three methods is the GPU-side work M6 defers until compute is
    available; the interface is the contract the rest of the loop is built to.
    """

    def __init__(self, model_name: str) -> None:
        self.model_name = model_name

    def sample(self, prompt: str, k: int) -> list[str]:
        """Draw k completions. The local model has no constrained decoding, so
        it emits prose/fences -- extraction is `extract_source` below, and
        whatever it yields is verified AS GIVEN (fenced junk earns a
        compile-zero, which is the correct training signal)."""
        raise NotImplementedError("GPU-side: load base_model and sample k completions")

    def sft(self, pairs: list[tuple[str, str]]) -> None:
        """One SFT pass over admitted (prompt, source) pairs."""
        raise NotImplementedError("GPU-side: a stock SFTTrainer step")

    def save(self, path: str) -> None:
        raise NotImplementedError("GPU-side: checkpoint the weights")


def extract_source(completion: str) -> str:
    """Pull a stratlite program out of a raw completion. A local sampler emits
    fences and prose; take the first ```-fenced block if present, else the raw
    text. The reward oracle is strict on purpose, so this stays a light
    convenience -- it never tries to REPAIR a program, only to unwrap it."""
    if "```" in completion:
        parts = completion.split("```")
        if len(parts) >= 3:
            body = parts[1]
            # drop an optional language tag on the fence's first line
            return body.split("\n", 1)[-1] if "\n" in body else body
    return completion


def score(cfg: Config, rollouts: list[Rollout]) -> dict:
    """Shell out to the tested reward oracle. This is the whole Rust boundary:
    JSONL of {id, source} in, JSONL of reward records out, same order, by id."""
    pool = "\n".join(json.dumps({"id": r.id, "source": r.source}) for r in rollouts)
    proc = subprocess.run(
        [cfg.s5_bin, "reward", "/dev/stdin", *cfg.candles],
        input=pool,
        capture_output=True,
        text=True,
        check=True,
    )
    return parse_rewards(proc.stdout)


def run(cfg: Config) -> None:
    policy = Policy(cfg.base_model)
    for r in range(cfg.rounds):
        rollouts: list[Rollout] = []
        for style in STYLES:
            for j, completion in enumerate(policy.sample(style, cfg.samples_per_style)):
                rollouts.append(Rollout(id=f"r{r}s{STYLES.index(style)}n{j}", style=style, source=extract_source(completion)))
        rewards = score(cfg, rollouts)
        admitted, stats = build_admission_set(
            rollouts, rewards, per_key_cap=cfg.per_key_cap, per_style_frac=cfg.per_style_frac
        )
        # The histogram IS the learning curve; the fuel spectrum is the evidence
        # for whether the termination bound is load-bearing on this task.
        print(f"round {r}: histogram={dict(stats.histogram)} admitted={len(admitted)} "
              f"distinct_nkeys={stats.distinct_nkeys} fuel[min,max]="
              f"{(stats.fuel_spectrum[0], stats.fuel_spectrum[-1]) if stats.fuel_spectrum else None}")
        policy.sft([(ro.style, ro.source) for ro in admitted])
        policy.save(f"checkpoints/C{r}")


if __name__ == "__main__":
    raise SystemExit(
        "train.py needs a GPU (torch/transformers) and pinned candles; it is not "
        "run in CI. The guards it relies on are tested in test_select.py."
    )
