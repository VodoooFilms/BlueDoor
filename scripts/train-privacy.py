#!/usr/bin/env python3
"""Local supervised QLoRA, exact production prompts, fixed validation, no uploads."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import time

os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
os.environ.setdefault("HF_HUB_OFFLINE", "1")
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
from mlx.utils import tree_flatten
import numpy as np
from mlx_lm.utils import load
from mlx_lm.tuner.trainer import TrainingArgs, TrainingCallback, train, default_loss
from mlx_lm.tuner.utils import linear_to_lora_layers, print_trainable_parameters

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / "artifacts/posttraining"
parser = argparse.ArgumentParser()
parser.add_argument("--epochs", type=int, default=1)
parser.add_argument("--batch-size", type=int, default=1)
parser.add_argument("--accumulation", type=int, default=8)
args = parser.parse_args()
assert args.epochs >= 1
SEED = 20261003
np.random.seed(SEED)
mx.random.seed(SEED)
mx.set_cache_limit(256 * 1024**2)

manifest = json.loads((RUN / "data/manifest.json").read_text())
for split in ["train", "valid"]:
    path = RUN / f"data/{split}.jsonl"
    assert hashlib.sha256(path.read_bytes()).hexdigest() == manifest[f"{split}_sha256"]
for name, expected in manifest["frozen_test_sha256"].items():
    assert hashlib.sha256((ROOT / "artifacts/benchmark" / name).read_bytes()).hexdigest() == expected

model, tokenizer = load(RUN / "base-mlx-q4", lazy=False)
model.freeze()
lora = dict(rank=16, scale=16.0, dropout=0.05,
            keys=["self_attn.q_proj", "self_attn.k_proj", "self_attn.v_proj", "self_attn.o_proj"])
linear_to_lora_layers(model, -1, lora)
print_trainable_parameters(model)

def dataset(split):
    data = []
    with (RUN / f"data/{split}.jsonl").open() as source:
        for line in source:
            row = json.loads(line)
            prompt = tokenizer.encode(row["prompt"], add_special_tokens=False)
            answer = tokenizer.encode(row["completion"], add_special_tokens=False) + [tokenizer.eos_token_id]
            assert len(prompt) == row["prompt_tokens"]
            assert len(prompt) + len(answer) == row["token_count"] <= manifest["max_tokens"]
            # NumPy int32 avoids retaining millions of boxed Python integers.
            data.append((np.asarray(prompt + answer, dtype=np.int32), len(prompt)))
    return data

training, validation = dataset("train"), dataset("valid")
# The first 384 positions are inside the fixed instruction for every example.
# Keep them in attention, but avoid materializing their 151k-vocabulary logits:
# their loss is exactly zero under completion masking. This reduces peak RAM.
PREFIX = 384
assert all(offset > PREFIX for _, offset in training + validation)
def completion_loss(model, batch, lengths):
    hidden = model.model(batch[:, :-1])[:, PREFIX:]
    logits = model.model.embed_tokens.as_linear(hidden) if model.args.tie_word_embeddings else model.lm_head(hidden)
    targets = batch[:, PREFIX+1:]
    positions = mx.arange(PREFIX+1, batch.shape[1])
    mask = (positions >= lengths[:, 0:1]) & (positions <= lengths[:, 1:])
    count = mask.sum()
    loss = (nn.losses.cross_entropy(logits, targets) * mask).astype(mx.float32).sum() / count
    return loss, count

# Verify the memory optimization against MLX's unmodified objective on real
# short and long examples before any optimizer update.
model.eval()
for tokens, offset in [min(validation, key=lambda r:len(r[0])), max(validation, key=lambda r:len(r[0]))]:
    batch, lengths = mx.array(tokens[None, :]), mx.array([[offset, len(tokens)]])
    expected, expected_count = default_loss(model, batch, lengths)
    mx.eval(expected, expected_count)
    expected_value, expected_n = expected.item(), expected_count.item()
    del expected, expected_count
    mx.clear_cache()
    actual, actual_count = completion_loss(model, batch, lengths)
    mx.eval(actual, actual_count)
    assert expected_n == actual_count.item()
    assert abs(expected_value-actual.item()) < 1e-5, (expected_value, actual.item())
    mx.clear_cache()
print("PASS completion objective equals the upstream loss on short/long validation examples", flush=True)
original_train, original_valid = len(training), len(validation)
multiple = args.batch_size * args.accumulation
while len(training) % multiple: training.append(training[len(training) - original_train])
while len(validation) % args.batch_size: validation.append(validation[len(validation) - original_valid])
iterations = len(training) // args.batch_size * args.epochs
updates = iterations // args.accumulation
warmup = min(100, max(1, updates // 20))
schedule = optim.join_schedules([
    optim.linear_schedule(1e-6, 1e-4, warmup),
    optim.cosine_decay(1e-4, max(1, updates - warmup), end=1e-5),
], [warmup])
optimizer = optim.AdamW(learning_rate=schedule, weight_decay=0.01)
adapters = RUN / "adapters"
best = RUN / "best-adapters"
adapters.mkdir(exist_ok=True)
best.mkdir(exist_ok=True)
config = dict(model=str(RUN / "base-mlx-q4"), fine_tune_type="lora", num_layers=-1,
              lora_parameters=lora, seed=SEED, epochs=args.epochs, iterations=iterations,
              batch_size=args.batch_size, grad_accumulation_steps=args.accumulation,
              original_train=original_train, original_valid=original_valid,
              padded_train=len(training), padded_valid=len(validation),
              max_seq_length=manifest["max_tokens"], prompt_sha256=manifest["prompt_sha256"],
              training_sha256=manifest["train_sha256"], validation_sha256=manifest["valid_sha256"])
for folder in [adapters, best]:
    (folder / "adapter_config.json").write_text(json.dumps(config, indent=2))

class Progress(TrainingCallback):
    def __init__(self):
        self.best_loss = float("inf")
        self.started = time.time()
        self.latest = {"phase":"training", "iterations":iterations, "config":config}
    def record(self, kind, values):
        assert all(math.isfinite(float(v)) for v in values.values() if isinstance(v, (int,float)))
        event = dict(kind=kind, elapsed_seconds=time.time()-self.started, **values)
        with (RUN / "metrics.jsonl").open("a") as out: out.write(json.dumps(event)+"\n")
        self.latest.update(event)
        temporary = RUN / "status.tmp"
        temporary.write_text(json.dumps(self.latest, indent=2))
        temporary.replace(RUN / "status.json")
        print(json.dumps(event), flush=True)
    def on_train_loss_report(self, values):
        self.record("train", values)
    def on_val_loss_report(self, values):
        loss = values["val_loss"]
        if loss < self.best_loss:
            self.best_loss = loss
            mx.save_safetensors(str(best / "adapters.safetensors"), dict(tree_flatten(model.trainable_parameters())))
            (best / "selection.json").write_text(json.dumps(values, indent=2))
        self.record("validation", {**values,"best_loss":self.best_loss})

progress = Progress()
print(json.dumps(config, indent=2), flush=True)
try:
    train(model=model, optimizer=optimizer, train_dataset=training, val_dataset=validation,
        loss=completion_loss,
        args=TrainingArgs(batch_size=args.batch_size, iters=iterations, val_batches=-1,
            steps_per_report=10, steps_per_eval=1000, steps_per_save=1000,
            max_seq_length=manifest["max_tokens"], adapter_file=str(adapters/"adapters.safetensors"),
            grad_checkpoint=True, grad_accumulation_steps=args.accumulation,
            clear_cache_threshold=256*1024**2), training_callback=progress)
    # Upstream evaluates immediately BEFORE its final update. Evaluate the actual
    # final weights too, with the same full held-out validation partition.
    from mlx_lm.tuner.trainer import evaluate
    loss = evaluate(model, validation, batch_size=args.batch_size, num_batches=-1, loss=completion_loss,
                    max_seq_length=manifest["max_tokens"])
    progress.on_val_loss_report(dict(iteration=iterations, val_loss=loss, val_time=0.0))
    progress.latest["phase"] = "trained"
    progress.record("complete", dict(iteration=iterations))
except BaseException as error:
    progress.latest["phase"] = "failed"
    progress.latest["error"] = repr(error)
    progress.record("failed", {})
    raise
