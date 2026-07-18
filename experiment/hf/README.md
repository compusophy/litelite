# experiment/hf — publish the fine-tuned models + datasets to the Hugging Face Hub

Hugging Face is a natural home for this work — arguably better than a PDF venue,
because the tangible artifacts of a fine-tuning result are the **models** and
**datasets**, which the Hub hosts natively and anyone can download and verify.

## What can be published

**Two models** (both `Qwen/Qwen3-0.6B` fine-tuned verifier-only, Apache-2.0):

| repo | what | headline |
|---|---|---|
| `prooflite-qwen3-0.6b` | generates `prooflite` (total compute language) | base 3.5% → **94.5%** rich, ~100% novel |
| `stratlite-qwen3-0.6b` | generates `stratlite` (strategy language) | base 0% → **100%** compile / ~96% held-out gate-clear |

Model cards are `card_prooflite.md` and `card_stratlite.md` (honest scope
included — no edge claim for stratlite, cross-model generality untested for both).

**One dataset** (`litelite-benchmarks`, optional `--datasets`): the cold-start
corpora, the scored benchmark pools (base/c5/c6/c7/c8 for prooflite, base/c7 for
stratlite), and the pinned candles — everything needed to reproduce the numbers.

## How to publish (needs YOUR account)

Nothing here uploads anything on its own. To push, from `experiment/hf/`:

    pip install huggingface_hub
    export HF_TOKEN=hf_xxx        # a WRITE token: huggingface.co/settings/tokens
    python publish.py <your-hf-username>              # the two models
    python publish.py <your-hf-username> --datasets   # also the datasets

`publish.py` templates the cards with your namespace, generates the prompt card
each model needs (`p6 card` / `s5 card`), bundles it, and pushes each checkpoint
folder. The checkpoints live in `../train/checkpoints_prooflite/C6` and
`../train/checkpoints/C7` (1.2 GB each; git-ignored, local only).

## Paper without arXiv

If arXiv endorsement is a blocker, the paper still has a home: `paper/paper.pdf`
is committed in the GitHub repo, and each model card links to it. HF "Papers"
indexes arXiv IDs, so a formal HF paper page would still want an arXiv id later —
but the models + datasets + linked PDF stand on their own as a citable release.
