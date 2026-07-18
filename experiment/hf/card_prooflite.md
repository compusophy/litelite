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
- prooflite
---

# prooflite-qwen3-0.6b — a verifier-only fine-tune of Qwen3-0.6B

`Qwen/Qwen3-0.6B` fine-tuned to generate **prooflite** — a tiny, *total*,
fuel-bounded compute language (one program computes, prints, and always halts) —
using ONLY a deterministic verifier as the training reward. No teacher model, no
API, no human labels.

## The result

| | base Qwen3-0.6B | this checkpoint (C6) |
|---|---|---|
| writes a RICH prooflite program (runs clean, prints ≥3 distinct lines, ≥30 fuel) | **3.5%** (9/256) | **94.5%** (242/256) |
| ...and that program is NOVEL (source-canonical key ∉ the training corpus) | — | **100%** (242/242) |
| parses at all | 23.4% | 99.2% |

Base Qwen3-0.6B recognizes the C-like surface enough to parse prooflite ~23% of
the time, but writes a *rich* program only 3.5%. Verifier-only self-play lifts
that to ~95%, and ~100% of the rich programs are source-canonically absent from
the 174-program cold-start corpus — the model learned to **write** prooflite, not
to recall the examples.

## How it was trained

Rejection-sampling self-play (expert iteration): sample from the current
checkpoint, score every program with the kit's verifier — a four-rung validity
ladder `compile → run → gate → ok` — admit the `ok`-rung survivors (deduped by a
source-canonical key, per-style capped to resist mode-collapse), SFT, repeat.
Nine rounds. This generalizes the tempo-x402 recipe (compiler-verified
self-play) off `rustc` onto a purpose-built language.

**This is checkpoint C6, selected at the diversity peak**: admitted distinct
programs peak at round 6 (823) then narrow (750, 686) as the policy trades
breadth for reward. C6 is where the generator is most diverse.

## Usage

```python
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

REPO = "{NAMESPACE}/prooflite-qwen3-0.6b"
tok = AutoTokenizer.from_pretrained(REPO)
model = AutoModelForCausalLM.from_pretrained(REPO, dtype=torch.float16).to("cuda").eval()

CARD = open("p6_card.txt").read()   # bundled in this repo (also `p6 card` in the source)
style = "a program that uses a repeat loop with a var accumulator to build up a total"
msgs = [{"role": "user", "content": CARD + "\n\n" + style}]
# Qwen3 emits a <think> block by default; suppress it (older tokenizers reject
# the kwarg, so fall back). tokenize=False then a separate tokenizer call is the
# trainer's exact, tested generation path.
try:
    prompt = tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True, enable_thinking=False)
except TypeError:
    prompt = tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True)
ids = tok(prompt, return_tensors="pt").to(model.device)
out = model.generate(**ids, do_sample=True, temperature=0.9, top_p=0.95,
                     max_new_tokens=256, pad_token_id=tok.pad_token_id)
print(tok.decode(out[0][ids["input_ids"].shape[1]:], skip_special_tokens=True))
```

Verify a generated program with the kit's engine: `p6 eval <pool.jsonl>` scores
the validity ladder; `p6 novelty <pool> <corpus>` checks it against the corpus.

## What this does NOT establish

- Not that every rich program is *interesting* — only well-formed, terminating,
  and varied output.
- **Generality across models is untested** — only Qwen3-0.6B was fine-tuned.
- Novelty is measured against the human cold-start corpus, not the model's own
  self-play programs, so it rules out memorizing the 174 examples, not
  reproduction from the larger self-generated training set.

## Source & paper

- Kit, verifier, and trainer: <https://github.com/compusophy/litelite>
- Paper (result in §5.7, the diversity-peak limit in §6.6): `paper/paper.pdf`
  in that repo.

## License

Apache-2.0, inherited from the `Qwen/Qwen3-0.6B` base model.
