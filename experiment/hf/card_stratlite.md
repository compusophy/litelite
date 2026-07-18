---
license: apache-2.0
base_model: Qwen/Qwen3-0.6B
library_name: transformers
pipeline_tag: text-generation
language:
- en
tags:
- code-generation
- verifier-guided
- self-play
- expert-iteration
- purpose-sized-languages
- stratlite
- trading-strategies
---

# stratlite-qwen3-0.6b — a verifier-only fine-tune of Qwen3-0.6B

`Qwen/Qwen3-0.6B` fine-tuned to generate **stratlite** — a small, total strategy
language whose backtest reduces to one reproducible integer and which has no name
for a future bar (no look-ahead by construction) — using ONLY a deterministic
verifier as the training reward. No teacher model, no API, no human labels.

## The result

Conditional metric: compile rate over the pool, then — among compilers — the
held-out gate-clear rate (a valid, actively-trading strategy), on three windows.

| window | base compile / gate-clear | this checkpoint (C7) |
|---|---|---|
| BTCUSDT Jan 2024 (train reward window) | 0.0% / 0.0% | 100.0% / 95.7% |
| BTCUSDT Jun 2024 (distant, 5-month embargo) | 0.0% / 0.0% | 100.0% / 96.5% |
| ETHUSDT Jun 2024 (cross-asset, never trained on) | 0.0% / 0.0% | 100.0% / 96.1% |

`stratlite` exists in no pretraining corpus, so base Qwen3-0.6B produces **zero**
valid programs across 256 attempts on every window — a measured-zero floor.
Verifier-only self-play lifts that to 100% compile and ~96% held-out gate-clear,
holding across a five-month embargo and a never-trained asset. Of C7's 251
gate-clearing programs, 250 (99.6%) are source-canonically absent from the
134-program cold-start (committed) corpus — learned, not memorized.

## How it was trained

Rejection-sampling self-play (expert iteration) with the deterministic verifier
as reward — the four-rung validity ladder `Compile → Run → Gate → Ok`. **Train
PnL is deliberately excluded from the reward**, so this generator carries no edge
claim: the verifier certifies that a strategy is well-formed and active, never
that it is profitable. Eight rounds; C7 selected on rich-rate saturation.

## Important scope

The near-zero train-vs-held-out gap (2.3 / 1.2 / 1.6 points) marks the lift
honestly as **grammar competence, not out-of-sample edge**. Picking a profitable
strategy from this now-competent generator is a separate, downstream job. This is
a validity result, not a trading system, and must not be used as one.

## Usage

```python
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

REPO = "{NAMESPACE}/stratlite-qwen3-0.6b"
tok = AutoTokenizer.from_pretrained(REPO)
model = AutoModelForCausalLM.from_pretrained(REPO, torch_dtype=torch.float16).to("cuda").eval()

CARD = open("s5_card.txt").read()   # bundled in this repo (also `s5 card` in the source)
style = "a momentum strategy that goes long on a fast/slow moving-average cross"
# Training-time format: the language card is the SYSTEM turn, the style a
# one-line user instruction. Qwen3 emits a <think> block by default; suppress it
# (older tokenizers reject the kwarg, so fall back). tokenize=False then a
# separate tokenizer call mirrors the trainer's generation path.
msgs = [
    {"role": "system", "content": CARD},
    {"role": "user", "content": f"Write {style}. Emit ONE stratlite program and nothing else."},
]
try:
    prompt = tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True, enable_thinking=False)
except TypeError:
    prompt = tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True)
ids = tok(prompt, return_tensors="pt").to(model.device)
out = model.generate(**ids, do_sample=True, temperature=0.9, top_p=0.95,
                     max_new_tokens=256, pad_token_id=tok.pad_token_id)
print(tok.decode(out[0][ids["input_ids"].shape[1]:], skip_special_tokens=True))
```

Verify a generated strategy with the kit's engine: `s5 eval <pool.jsonl>
data/<window>.csv` scores compile + held-out gate-clear on pinned candles.

## What this does NOT establish

- **No edge.** The reward is validity-only; a null-PnL result falsifies nothing.
- **Generality across models is untested** — only Qwen3-0.6B was fine-tuned.
- **Novelty scope**: the 99.6% is measured against the human cold-start corpus
  only, not the model's own self-play programs — it rules out memorizing the 134
  examples, not reproduction from the larger self-generated training set.
- The single-month benchmark has no out-of-sample teeth (the held-out window is
  no harder than train); the competence generalizes because the *language* is
  regime- and asset-independent, not because the model found generalizing edge.

## Source & paper

- Kit, verifier, and trainer: <https://github.com/compusophy/litelite>
- Paper (result in §5.6): `paper/paper.pdf` in that repo.

## License

Apache-2.0, inherited from the `Qwen/Qwen3-0.6B` base model.
