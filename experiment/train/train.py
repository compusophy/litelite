"""M6 rejection-sampling SFT loop (expert iteration) -- the tempo-x402 recipe.

    ┌─ sample K programs per style from the current policy   (torch)
    │  write rollouts {id, style, source}
    ├─ score them with the verifier                          `s5 reward`
    ├─ build the SFT admission set                           admission.py (tested)
    │  keep Ok-rung only, dedup by nkey, cap per style
    ├─ SFT the policy on the admitted {prompt, source}       (torch)
    └─ checkpoint, log the round's Reject histogram + fuel spectrum
       repeat until the held-out gate-clear lift plateaus

Runs on a single 24GB GPU (a 3090 is ample). The default is a full fine-tune of
the small dense Qwen3-0.6B -- ~8-9GB peak, tiny context, CPU-free reward. For 4B
use LoRA. Model sizing (Qwen3-Coder is all too big; use the dense line) and the
non-thinking detail are in README.md.

The load-bearing anti-collapse logic (admission + extraction) lives in
admission.py and is TESTED without a GPU -- that is where the reward hacks
live. (It must never be named select.py: the script's directory shadows the
stdlib `select` module, which subprocess/selectors import on POSIX.)
The torch surface here is deliberately thin and isolated behind `Policy`. The
prompt card and styles are read from the `s5` binary (`s5 card` / `s5 styles`)
rather than copied, so the prompt the model learns and the language the
verifier enforces stay ONE artifact.

Reproducibility, honestly: the model is NOT reproducible (sampling is
stochastic; a fine-tune is not bit-identical across hardware). What reproduces
from a command is the REWARD -- `s5 reward` is deterministic -- so the run
COMMITS its checkpoints and rollouts as artifacts rather than pretending to
regenerate them.
"""

from __future__ import annotations

import json
import random
import subprocess
from dataclasses import dataclass

from admission import (
    Rollout,
    build_admission_set,
    extract_source,
    parse_rewards,
)


@dataclass
class Config:
    # A small DENSE current-gen model. Qwen3-Coder is all big (30B-A3B MoE and
    # up); the small dense Qwen3 line is the right size for a 24GB card, and a
    # general model is fine — even preferable — for a tiny DSL the model learns
    # by verified self-play, not by pretraining. Swap freely; it is just a
    # HuggingFace id. Qwen3 THINKS by default, so `_prompt` disables it.
    base_model: str = "Qwen/Qwen3-0.6B"
    s5_bin: str = "../target/release/s5"  # the tested reward oracle + card source
    candles: tuple[str, ...] = ()  # pinned klines CSVs (train+val windows)
    out_dir: str = "checkpoints"
    # Generation
    samples_per_style: int = 128  # K
    rounds: int = 12
    max_new_tokens: int = 256  # a stratlite program is short
    temperature: float = 0.9  # diversity comes from sampling + the styles
    top_p: float = 0.95
    sample_batch: int = 64
    # SFT
    sft_epochs: int = 1
    sft_batch: int = 8
    lr: float = 1e-5
    # Anti-collapse (passed to admission.py)
    per_key_cap: int = 2
    per_style_frac: float = 0.5
    # Cold start: SFT on the committed corpus survivors first. Mandatory in
    # practice — the base model's yield on the card is ~0%, so round 1 of
    # rejection sampling would admit nothing (measured: 8/8 compile-fails).
    corpus: str = ""
    cold_epochs: int = 3
    # Resume: set base_model to a saved checkpoint dir and corpus="" to skip
    # cold start; round_offset keeps checkpoint numbering continuous (C2, C3…)
    # so a resumed run does not clobber the checkpoints it started from.
    round_offset: int = 0
    # Stratlite needs a candle window for its reward; prooflite (p6) does not —
    # its reward is intrinsic to a program's execution.
    needs_candles: bool = True
    # Hardware
    device: str = "cuda"
    dtype: str = "bfloat16"  # Ampere (3090) supports bf16
    seed: int = 0  # seeds torch/python; does NOT make a fine-tune reproducible


def _run(cfg: Config, *args: str, stdin: str | None = None) -> str:
    proc = subprocess.run(
        [cfg.s5_bin, *args], input=stdin, capture_output=True, text=True, check=True
    )
    return proc.stdout


def card(cfg: Config) -> str:
    """The prompt card -- stratlite::REFERENCE, straight from the binary."""
    return _run(cfg, "card")


def styles(cfg: Config) -> list[str]:
    return [s for s in _run(cfg, "styles").splitlines() if s.strip()]


def score(cfg: Config, rollouts: list[Rollout]) -> dict:
    """The whole Rust boundary: JSONL {id, source} in, reward records out, by id."""
    pool = "\n".join(json.dumps({"id": r.id, "source": r.source}) for r in rollouts)
    # "-" = stdin: portable ("/dev/stdin" does not exist on Windows, the training box).
    out = _run(cfg, "reward", "-", *cfg.candles, stdin=pool)
    return parse_rewards(out)


class Policy:
    """The ONLY torch-dependent surface. Imports are lazy so the module loads --
    and admission.py's guards test -- without torch installed."""

    def __init__(self, cfg: Config, system: str) -> None:
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer

        random.seed(cfg.seed)
        torch.manual_seed(cfg.seed)
        self.cfg = cfg
        self.system = system
        self.tok = AutoTokenizer.from_pretrained(cfg.base_model)
        if self.tok.pad_token is None:
            self.tok.pad_token = self.tok.eos_token
        self.model = AutoModelForCausalLM.from_pretrained(
            cfg.base_model, dtype=getattr(torch, cfg.dtype)
        ).to(cfg.device)
        self.opt = torch.optim.AdamW(self.model.parameters(), lr=cfg.lr)

    def _prompt(self, user: str) -> str:
        """The chat template, with the language card as the system turn and
        add_generation_prompt so the model completes the assistant turn. Qwen3
        thinks by default; `enable_thinking=False` suppresses the <think> block
        so the completion is the program, not reasoning about it. The fallback
        keeps a non-Qwen3 model (whose template lacks that kwarg) working."""
        msgs = [
            {"role": "system", "content": self.system},
            {"role": "user", "content": f"Write {user}. Emit ONE stratlite program and nothing else."},
        ]
        try:
            return self.tok.apply_chat_template(
                msgs, tokenize=False, add_generation_prompt=True, enable_thinking=False
            )
        except TypeError:
            return self.tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True)

    def sample(self, user: str, k: int) -> list[str]:
        import torch

        self.model.eval()
        prompt = self._prompt(user)
        ids = self.tok(prompt, return_tensors="pt").to(self.cfg.device)
        plen = ids["input_ids"].shape[1]
        out: list[str] = []
        for start in range(0, k, self.cfg.sample_batch):
            n = min(self.cfg.sample_batch, k - start)
            with torch.no_grad():
                gen = self.model.generate(
                    **ids,
                    do_sample=True,
                    temperature=self.cfg.temperature,
                    top_p=self.cfg.top_p,
                    max_new_tokens=self.cfg.max_new_tokens,
                    num_return_sequences=n,
                    pad_token_id=self.tok.pad_token_id,
                )
            for g in gen:
                out.append(self.tok.decode(g[plen:], skip_special_tokens=True))
        return out

    def sft(self, pairs: list[tuple[str, str]]) -> None:
        """One or more passes over admitted (user_style, source) pairs. Loss is
        masked to the COMPLETION only (prompt tokens are -100), so the model
        learns to produce the program, not to re-emit the card.

        Memory: generation leaves a large KV cache the caching allocator holds;
        free it before SFT allocates gradients. Gradient checkpointing then cuts
        activation memory (recompute in backward) so long programs — prooflite's
        loops run to hundreds of tokens — fit a 24GB card without OOM."""
        import torch

        if not pairs:
            return
        torch.cuda.empty_cache()
        self.model.train()
        self.model.gradient_checkpointing_enable()
        self.model.config.use_cache = False  # incompatible with checkpointing
        for _ in range(self.cfg.sft_epochs):
            random.shuffle(pairs)
            for i in range(0, len(pairs), self.cfg.sft_batch):
                batch = pairs[i : i + self.cfg.sft_batch]
                input_ids, labels = self._collate(batch)
                loss = self.model(input_ids=input_ids, labels=labels).loss
                loss.backward()
                self.opt.step()
                self.opt.zero_grad()
        self.model.gradient_checkpointing_disable()
        self.model.config.use_cache = True  # restore for fast generation

    def _collate(self, batch: list[tuple[str, str]]):
        import torch

        rows, label_rows = [], []
        for user, source in batch:
            prompt = self._prompt(user)
            p_ids = self.tok(prompt, add_special_tokens=False)["input_ids"]
            c_ids = self.tok(source + self.tok.eos_token, add_special_tokens=False)["input_ids"]
            ids = p_ids + c_ids
            labels = [-100] * len(p_ids) + c_ids  # mask the prompt
            rows.append(ids)
            label_rows.append(labels)
        width = max(len(r) for r in rows)
        pad = self.tok.pad_token_id
        input_ids = torch.tensor([r + [pad] * (width - len(r)) for r in rows], device=self.cfg.device)
        labels = torch.tensor([r + [-100] * (width - len(r)) for r in label_rows], device=self.cfg.device)
        return input_ids, labels

    def save(self, path: str) -> None:
        self.model.save_pretrained(path)
        self.tok.save_pretrained(path)


def cold_start(cfg: Config, policy: Policy, style_list: list[str]) -> None:
    """SFT on the committed corpus survivors, mapped to the full style
    sentences so training prompts match sampling prompts exactly. A corpus row
    carries either a "style_idx" (language-agnostic, preferred) or a stratlite
    family key in "style" (the original corpus format)."""
    fam = {"trend": 0, "meanrev": 1, "breakout": 2, "momentum": 3, "stateful": 4, "combo": 6}

    def style_of(r: dict) -> str:
        idx = r["style_idx"] if "style_idx" in r else fam.get(r.get("style", ""), 6)
        return style_list[idx]

    rows = [json.loads(l) for l in open(cfg.corpus, encoding="utf-8") if l.strip()]
    rollouts = [Rollout(r["id"], style_of(r), r["source"]) for r in rows]
    rewards = score(cfg, rollouts)
    admitted, stats = build_admission_set(
        rollouts, rewards, per_key_cap=cfg.per_key_cap, per_style_frac=cfg.per_style_frac
    )
    print(f"cold start: {len(admitted)}/{len(rollouts)} corpus programs admitted "
          f"(histogram={dict(stats.histogram)})", flush=True)
    pairs = [(ro.style, ro.source) for ro in admitted]
    for _ in range(cfg.cold_epochs):
        policy.sft(list(pairs))


def run(cfg: Config) -> None:
    if cfg.needs_candles and not cfg.candles:
        raise SystemExit("wire pinned candle CSVs into Config.candles before running")
    system, style_list = card(cfg), styles(cfg)
    policy = Policy(cfg, system)
    if cfg.corpus:
        cold_start(cfg, policy, style_list)
        policy.save(f"{cfg.out_dir}/Cinit")
    for step in range(cfg.rounds):
        r = step + cfg.round_offset
        rollouts: list[Rollout] = []
        for si, style in enumerate(style_list):
            for j, completion in enumerate(policy.sample(style, cfg.samples_per_style)):
                rollouts.append(Rollout(id=f"r{r}s{si}n{j}", style=style, source=extract_source(completion)))
        rewards = score(cfg, rollouts)
        admitted, stats = build_admission_set(
            rollouts, rewards, per_key_cap=cfg.per_key_cap, per_style_frac=cfg.per_style_frac
        )
        # The histogram IS the learning curve; the fuel spectrum is the evidence
        # for whether the termination bound is load-bearing on this task.
        fuel = (stats.fuel_spectrum[0], stats.fuel_spectrum[-1]) if stats.fuel_spectrum else None
        print(
            f"round {r}: histogram={dict(stats.histogram)} admitted={len(admitted)} "
            f"distinct_nkeys={stats.distinct_nkeys} fuel[min,max]={fuel}",
            flush=True,
        )
        policy.sft([(ro.style, ro.source) for ro in admitted])
        policy.save(f"{cfg.out_dir}/C{r}")


if __name__ == "__main__":
    import sys

    # python3 train.py <candles.csv>... — reward window(s); corpus cold start on.
    run(Config(candles=tuple(sys.argv[1:]), corpus="../corpus/seed.jsonl"))
