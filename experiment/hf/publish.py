"""Publish the litelite fine-tuned models (and optionally the datasets) to the
Hugging Face Hub.

    export HF_TOKEN=hf_xxx          # a WRITE token: huggingface.co/settings/tokens
    python publish.py <your-hf-username>              # push the two models
    python publish.py <your-hf-username> --datasets   # also push corpora + pools

NOTHING is uploaded until you run this yourself with your own token and
namespace. The model cards (card_*.md) are templated with {NAMESPACE} at upload
time, and the prompt card each model needs (`p6 card` / `s5 card`) is generated
from the built binary and bundled into the repo, so the usage snippet works.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EXP = HERE.parent                      # experiment/
TRAIN = EXP / "train"
P6 = EXP / "proofbench" / "target" / "release" / "p6.exe"
S5 = EXP / "target" / "release" / "s5.exe"

# (repo name, checkpoint dir, model card, prompt-card binary, prompt-card filename)
MODELS = [
    ("prooflite-qwen3-0.6b", TRAIN / "checkpoints_prooflite" / "C6",
     HERE / "card_prooflite.md", P6, "p6_card.txt"),
    ("stratlite-qwen3-0.6b", TRAIN / "checkpoints" / "C7",
     HERE / "card_stratlite.md", S5, "s5_card.txt"),
]


def prompt_card(binary: Path) -> str:
    return subprocess.run([str(binary), "card"], capture_output=True, text=True,
                          check=True).stdout


def main() -> None:
    if len(sys.argv) < 2 or sys.argv[1].startswith("-"):
        raise SystemExit(__doc__)
    ns = sys.argv[1]
    token = os.environ.get("HF_TOKEN")
    if not token:
        raise SystemExit("set HF_TOKEN — a write token from huggingface.co/settings/tokens")

    from huggingface_hub import HfApi

    api = HfApi(token=token)
    for repo, ckpt, card, binary, cardfile in MODELS:
        if not ckpt.exists():
            print(f"SKIP {repo}: checkpoint {ckpt} not found")
            continue
        rid = f"{ns}/{repo}"
        api.create_repo(rid, repo_type="model", exist_ok=True)
        # Bundle the model card (as README.md) and the prompt card the usage
        # snippet reads, then push the whole checkpoint folder.
        (ckpt / "README.md").write_text(
            card.read_text(encoding="utf-8").replace("{NAMESPACE}", ns),
            encoding="utf-8", newline="\n")
        if binary.exists():
            (ckpt / cardfile).write_text(prompt_card(binary), encoding="utf-8", newline="\n")
        print(f"uploading {ckpt.name} -> {rid} ...")
        api.upload_folder(folder_path=str(ckpt), repo_id=rid, repo_type="model")
        print(f"  done: https://huggingface.co/{rid}")

    if "--datasets" in sys.argv:
        ds = f"{ns}/litelite-benchmarks"
        api.create_repo(ds, repo_type="dataset", exist_ok=True)
        proof = EXP / "proofbench"
        # prooflite: corpus + scored benchmark pools
        api.upload_folder(folder_path=str(proof / "results"), repo_id=ds,
                          repo_type="dataset", path_in_repo="prooflite/results")
        api.upload_file(path_or_fileobj=str(proof / "corpus" / "seed.jsonl"),
                        path_in_repo="prooflite/corpus_seed.jsonl",
                        repo_id=ds, repo_type="dataset")
        # stratlite: corpus + benchmark pools + pinned candles
        api.upload_file(path_or_fileobj=str(EXP / "corpus" / "seed.jsonl"),
                        path_in_repo="stratlite/corpus_seed.jsonl",
                        repo_id=ds, repo_type="dataset")
        api.upload_folder(folder_path=str(EXP / "results"), repo_id=ds,
                          repo_type="dataset", path_in_repo="stratlite/results")
        api.upload_folder(folder_path=str(EXP / "data"), repo_id=ds,
                          repo_type="dataset", path_in_repo="stratlite/candles")
        print(f"  done: https://huggingface.co/datasets/{ds}")

    print("\nAll pushes complete. Set authorship/description on the repo pages as desired.")


if __name__ == "__main__":
    main()
