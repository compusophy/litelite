# experiment/train — the M6 trainer

The Python side of M6 (design + honest limits: `../M6.md`). It fine-tunes a
small open-weights model to generate stratlite, using the kit's verifier as the
reward. It is OUTSIDE the kit's Cargo workspace because it takes deps
(torch/transformers) that would break the kit's zero-dep rule and its wasm32
gate — the same seam `experiment/` itself uses.

## What runs where

| File            | Deps        | Runs in CI? | Why                                            |
|-----------------|-------------|-------------|------------------------------------------------|
| `select.py`     | stdlib      | **yes**     | the anti-collapse guards + extraction — tested  |
| `test_select.py`| stdlib      | **yes**     | pins each guard against a named reward hack     |
| `train.py`      | torch, GPU  | no          | the sampling + SFT loop; runs on your GPU       |

The split is deliberate. The reward hacks the red-team named all live in *what
you train on*, and that decision is `select.py` — Ok-rung only, dedup by the
source-canonical `nkey`, cap each style's share. That is verifiable now, with no
GPU, and it is tested. The torch calls in `train.py` are isolated behind
`Policy`; CI byte-compiles the file (imports are lazy) but does not run it.

## Model — small, dense, current-gen

`Qwen3-Coder` is all large (30B-A3B MoE and up) — wrong size for a 24GB card.
The small **dense** Qwen3 line is the right fit, and a general model is fine —
even preferable — for a tiny DSL the model learns by verified self-play, not by
pretraining (tempo-x402 lifted a general 0.5B, not a code model). Qwen3 *thinks*
by default; `train.py` disables it (`enable_thinking=False`) so the completion
is the program, not reasoning. `base_model` is just a HuggingFace id — swap it.

| Model | Fits 24GB? | Config change |
|---|---|---|
| **Qwen/Qwen3-0.6B** (default) | yes, ~8–9GB | none — full fine-tune |
| Qwen/Qwen3-1.7B | yes | full FT (tighter); or LoRA |
| Qwen/Qwen3-4B | yes | LoRA (add `peft`; wrap the model in `Policy.__init__`) |

This workload is light on VRAM: a stratlite program is a few hundred tokens, the
model is small, and the reward runs on the CPU verifier, not the GPU. The cost
is sampling throughput (K×8 styles×rounds completions) — fine on a 3090; add
vLLM later if you want the rounds faster. 128GB of system RAM is plenty for the
rollout pools and dataset.

## The boundary to the verifier

`train.py` shells out to the tested reward oracle — nothing more:

```
s5 reward <pool.jsonl> <candles.csv>...   # {id,source} in → reward records out
```

`s5 reward` is deterministic, so a checkpoint's rewards (and its eval numbers)
reproduce from a command even though the model never can. That is where M6's
reproducibility lives; see `../M6.md`.

## Run it (on your machine)

```
# 1. build the reward oracle once (it is also the card/styles source of truth)
cd ..            && cargo build --release
cd experiment    && ./target/release/s5 data data/BTCUSDT-1h-2024-01.csv   # sanity

# 2. the trainer
cd train
python3 -m venv .venv && . .venv/bin/activate     # (Windows: .venv\Scripts\activate)
pip install -r requirements.txt                    # a CUDA torch build
python3 train.py ../data/BTCUSDT-1h-2024-01.csv    # candle CSVs as args
```

Each round prints its Reject histogram (the learning curve — mass should shift
right toward `ok`), the admitted-set size, the distinct-`nkey` count (watch for
collapse), and the fuel spectrum. Checkpoints land in `checkpoints/C{r}`.

Before trusting any lift, run the eval discipline in `../M6.md`: the metric is
the CONDITIONAL held-out gate-clear rate (among compilers), and you must first
confirm the base model has a non-zero train-vs-held-out gap, or the benchmark
has no out-of-sample teeth.

## Verify the guards (no GPU, seconds)

```
python3 test_select.py
```
