"""Publish the litelite fine-tuned models (and optionally the datasets) to the
Hugging Face Hub.

    pip install huggingface_hub
    export HF_TOKEN=hf_xxx          # a WRITE token: huggingface.co/settings/tokens
    python publish.py <your-hf-username>              # push the two models
    python publish.py <your-hf-username> --datasets   # also push corpora + pools

NOTHING is uploaded until you run this yourself with your own token + namespace.
The model cards (card_*.md) are templated with {NAMESPACE}, and each model's
prompt card (p6_card.txt / s5_card.txt, committed alongside — regenerate with
`p6 card` / `s5 card` if the language card changes) is bundled so the usage
snippet works out of the box, on any platform, with no build step.
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXP = HERE.parent

# (repo name, checkpoint dir, model card, bundled prompt-card file)
MODELS = [
    ("prooflite-qwen3-0.6b", EXP / "train" / "checkpoints_prooflite" / "C6",
     HERE / "card_prooflite.md", HERE / "p6_card.txt"),
    ("stratlite-qwen3-0.6b", EXP / "train" / "checkpoints" / "C7",
     HERE / "card_stratlite.md", HERE / "s5_card.txt"),
]

# What may be pushed from a checkpoint dir — never a stray optimizer state, log,
# notebook checkpoint, or dotfile lands in a permanent public repo.
ALLOW = ["*.safetensors", "*.json", "*.jinja", "README.md", "*_card.txt"]


def main() -> None:
    if len(sys.argv) < 2 or sys.argv[1].startswith("-"):
        raise SystemExit(__doc__)
    ns = sys.argv[1]
    token = os.environ.get("HF_TOKEN")
    if not token:
        raise SystemExit("set HF_TOKEN — a write token from huggingface.co/settings/tokens")
    try:
        from huggingface_hub import HfApi
    except ImportError:
        raise SystemExit("pip install huggingface_hub")

    api = HfApi(token=token)
    for repo, ckpt, card, cardfile in MODELS:
        if not ckpt.exists():
            print(f"SKIP {repo}: checkpoint {ckpt} not found")
            continue
        if not cardfile.exists():
            raise SystemExit(f"{repo}: prompt card {cardfile.name} missing — "
                             f"regenerate it (`p6 card` / `s5 card`) before publishing")
        # Stage the bundled files INTO the checkpoint dir BEFORE the repo exists,
        # so any failure aborts before anything public is created.
        (ckpt / "README.md").write_text(
            card.read_text(encoding="utf-8").replace("{NAMESPACE}", ns),
            encoding="utf-8", newline="\n")
        shutil.copyfile(cardfile, ckpt / cardfile.name)
        rid = f"{ns}/{repo}"
        api.create_repo(rid, repo_type="model", exist_ok=True)
        print(f"uploading {ckpt.name} -> {rid} ...")
        api.upload_folder(folder_path=str(ckpt), repo_id=rid, repo_type="model",
                          allow_patterns=ALLOW)
        print(f"  done: https://huggingface.co/{rid}")

    if "--datasets" in sys.argv:
        ds = f"{ns}/litelite-benchmarks"
        api.create_repo(ds, repo_type="dataset", exist_ok=True)
        # A top-level dataset card so the Hub page carries provenance + license.
        api.upload_file(path_or_fileobj=str(HERE / "dataset_card.md"),
                        path_in_repo="README.md", repo_id=ds, repo_type="dataset")
        proof = EXP / "proofbench"
        api.upload_folder(folder_path=str(proof / "results"), repo_id=ds,
                          repo_type="dataset", path_in_repo="prooflite/results")
        api.upload_file(path_or_fileobj=str(proof / "corpus" / "seed.jsonl"),
                        path_in_repo="prooflite/corpus_seed.jsonl",
                        repo_id=ds, repo_type="dataset")
        api.upload_file(path_or_fileobj=str(EXP / "corpus" / "seed.jsonl"),
                        path_in_repo="stratlite/corpus_seed.jsonl",
                        repo_id=ds, repo_type="dataset")
        api.upload_folder(folder_path=str(EXP / "results"), repo_id=ds,
                          repo_type="dataset", path_in_repo="stratlite/results")
        api.upload_folder(folder_path=str(EXP / "data"), repo_id=ds,
                          repo_type="dataset", path_in_repo="stratlite/candles")
        print(f"  done: https://huggingface.co/datasets/{ds}")

    print("\nAll pushes complete.")


if __name__ == "__main__":
    main()
