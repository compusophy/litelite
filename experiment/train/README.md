# experiment/train — the M6 trainer

The Python side of M6 (design + honest limits: `../M6.md`). It fine-tunes a
small open-weights model to generate stratlite, using the kit's verifier as the
reward. It is OUTSIDE the kit's Cargo workspace because it takes deps
(torch/transformers) that would break the kit's zero-dep rule and its wasm32
gate — the same seam `experiment/` itself uses.

## What runs where

| File            | Deps        | Runs in CI? | Why                                            |
|-----------------|-------------|-------------|------------------------------------------------|
| `select.py`     | stdlib      | **yes**     | the anti-collapse guards — the load-bearing bit |
| `test_select.py`| stdlib      | **yes**     | pins each guard against a named reward hack     |
| `train.py`      | torch, GPU  | no          | the sampling + SFT loop; GPU-side, deferred     |

The split is deliberate. The reward hacks the red-team named all live in *what
you train on*, and that decision is `select.py` — Ok-rung only, dedup by the
source-canonical `nkey`, cap each style's share. That is verifiable now, with no
GPU, and it is tested. The torch calls in `train.py` are isolated behind
`Policy` so the ML dependency is a thin shell, not the whole file.

## The boundary to the verifier

`train.py` shells out to the tested reward oracle — nothing more:

```
s5 reward <pool.jsonl> <candles.csv>...   # {id,source} in → reward records out
```

`s5 reward` is deterministic, so a checkpoint's rewards (and its eval numbers)
reproduce from a command even though the model never can. That is where M6's
reproducibility lives; see `../M6.md`.

## Run (given a GPU)

```
pip install -r requirements.txt
(cd .. && cargo build --release)          # build the s5 reward oracle
python3 train.py                          # needs candles wired into Config
```

## Verify the guards (no GPU)

```
python3 test_select.py
```
