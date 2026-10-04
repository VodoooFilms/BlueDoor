#!/usr/bin/env python3
"""Fetch only public, revision-pinned inputs for the local training recipe."""
import hashlib
import json
import os
from pathlib import Path
import urllib.request

os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
from huggingface_hub import snapshot_download, hf_hub_download

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / "artifacts/posttraining"
DATASETS = ROOT / "artifacts/datasets"
RUN.mkdir(parents=True, exist_ok=True)
DATASETS.mkdir(exist_ok=True)

snapshot_download("Qwen/Qwen3-1.7B", revision="70d244cc86ccca08cf5af4e1e306ecf908b1ad5e",
    local_dir=RUN/"base-hf", allow_patterns=["*.json", "*.safetensors", "*.txt", "LICENSE", "README.md"], token=False)
for language in ["Spanish", "English"]:
    hf_hub_download("gretelai/synthetic_pii_finance_multilingual", repo_type="dataset",
        revision="7b844d16738527a04264f50214cb426a4cea0897",
        filename=f"data/{language}_train-00000-of-00001.parquet", local_dir=RUN/"gretel", token=False)

for record, filename, local_name in [(4279323, "meddocan.zip", "meddocan.zip"),
                                      (20863452, "DialogPII.zip", "DialogPII.zip")]:
    with urllib.request.urlopen(f"https://zenodo.org/api/records/{record}", timeout=60) as response:
        metadata = json.load(response)
    (DATASETS/f"zenodo-{record}.json").write_text(json.dumps(metadata, indent=2))
    entry = next(f for f in metadata["files"] if f["key"] == filename)
    path = DATASETS/local_name
    if not path.exists():
        temporary = path.with_suffix(".downloading")
        with urllib.request.urlopen(entry["links"]["self"], timeout=120) as response, temporary.open("wb") as output:
            while chunk := response.read(1024*1024): output.write(chunk)
        temporary.replace(path)
    method, expected = entry["checksum"].split(":", 1)
    with path.open("rb") as source: actual = hashlib.file_digest(source, method).hexdigest()
    assert actual == expected, f"Dataset checksum mismatch: {path}"

revision = "ecfdc547f4a0955600cfe6ab98ba2a162207fcc0"
selection = RUN/"openpii-selection.json"
selected = RUN/"openpii-train-selected.jsonl"
if not selection.exists() or not selected.exists():
    url = f"https://huggingface.co/datasets/ai4privacy/pii-masking-openpii-1m/resolve/{revision}/data/train.jsonl"
    temporary = selected.with_suffix(".downloading")
    counts, scanned = {"es":0, "en":0}, 0
    with urllib.request.urlopen(url, timeout=120) as response, temporary.open("w") as out:
        for line in response:
            row = json.loads(line)
            scanned += 1
            language = row.get("language")
            if language in counts and counts[language] < 6000:
                out.write(json.dumps({k:row[k] for k in ["uid", "language", "source_text", "privacy_mask"]}, ensure_ascii=False)+"\n")
                counts[language] += 1
            if scanned % 20000 == 0: print(scanned, counts, flush=True)
            if min(counts.values()) == 6000: break
    assert min(counts.values()) == 6000
    temporary.replace(selected)
    selection.write_text(json.dumps(dict(repo="ai4privacy/pii-masking-openpii-1m", sha=revision,
                                        scanned=scanned, counts=counts, source_split="train"), indent=2))
print("Public model and training inputs are available locally")
