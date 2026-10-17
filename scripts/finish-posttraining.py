#!/usr/bin/env python3
"""Convert trained weights, evaluate the real app pipeline, and promote only gains."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / "artifacts/posttraining"
PYTHON = ROOT / ".build-cache/training-env/bin/python"
SDK = "/Library/Developer/CommandLineTools/SDKs/MacOSX14.5.sdk"
ENV = dict(os.environ, SDKROOT=SDK, CMAKE_OSX_SYSROOT=SDK, CC="/usr/bin/clang", CXX="/usr/bin/clang++",
           HF_HUB_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1", PYTHONUNBUFFERED="1")

def sha(path):
    with Path(path).open("rb") as f: return hashlib.file_digest(f, "sha256").hexdigest()

def stage(name, command, extra=None):
    (RUN / "pipeline-status.json").write_text(json.dumps(dict(phase=name, pid=os.getpid(), updated=time.time()), indent=2))
    print(name, flush=True)
    with (RUN / f"{name}.log").open("w") as log:
        subprocess.run([str(s) for s in command], cwd=ROOT, env={**ENV, **(extra or {})},
                       stdout=log, stderr=subprocess.STDOUT, check=True)

def finish():
    data_manifest = json.loads((RUN / "data/manifest.json").read_text())
    trained_config = json.loads((RUN / "best-adapters/adapter_config.json").read_text())
    assert data_manifest["train_sha256"] == trained_config["training_sha256"]
    assert data_manifest["prompt_sha256"] == trained_config["prompt_sha256"]
    for name, expected in data_manifest["frozen_test_sha256"].items():
        assert sha(ROOT / "artifacts/benchmark" / name) == expected, "Frozen evaluation data changed"
    for filename, expected in json.loads((RUN / "source-fingerprints.json").read_text()).items():
        assert sha(ROOT / filename) == expected, "Inference changed during training: repeat the baseline before comparison"
    selection = json.loads((RUN / "best-adapters/selection.json").read_text())
    assert selection["iteration"] > 0, "No trained checkpoint improved validation loss"
    # Merge learned deltas into the original base weights. Re-quantizing an
    # already dequantized Q4 base would introduce a second quantization error.
    stage("fuse", [PYTHON, "-m", "mlx_lm", "fuse", "--model", RUN/"base-hf",
                   "--adapter-path", RUN/"best-adapters", "--save-path", RUN/"fused-hf", "--dequantize"])
    llama = ROOT / ".build-cache/llama-posttraining"
    stage("convert-gguf", [PYTHON, llama/"convert_hf_to_gguf.py", RUN/"fused-hf",
                           "--outfile", RUN/"belegal-pii-v1-f16.gguf", "--outtype", "f16"])
    trained = RUN / "belegal-pii-v1-q4.gguf"
    stage("quantize", [llama/"build/bin/llama-quantize", RUN/"belegal-pii-v1-f16.gguf", trained, "Q4_K_M", "6"])
    trained_hash = sha(trained)
    comparisons = {}
    for name, fixture, baseline in [("openpii", "openpii-validation.jsonl", "baseline"),
                                     ("gretel", "gretel-validation.jsonl", "gretel")]:
        output = f"trained-{name}-results.json"
        stage(f"evaluate-{name}", ["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--lib",
              "measure_dataset_workflow", "--", "--ignored", "--nocapture"],
              dict(BLUEDOOR_BENCHMARK_FILE=fixture, BLUEDOOR_BENCHMARK_OUTPUT=output,
                   BLUEDOOR_BENCHMARK_MODEL=str(trained), BLUEDOOR_BENCHMARK_MODEL_SHA256=trained_hash))
        stage(f"score-{name}", [PYTHON, "scripts/score-privacy-benchmark.py", ROOT/"artifacts/benchmark"/output])
        before = json.loads((ROOT/f"artifacts/benchmark/{baseline}-score.json").read_text())
        after = json.loads((ROOT/f"artifacts/benchmark/trained-{name}-score.json").read_text())
        assert before["documents"] == after["documents"] and before["goldAnnotations"] == after["goldAnnotations"]
        # Every original label stays in the comparison, including policy mismatches.
        assert before["byLabel"].keys() == after["byLabel"].keys()
        improved = after["visible"] < before["visible"] and after["incompleteDocuments"] <= before["incompleteDocuments"] and after["errors"] <= before["errors"]
        # Keep every label in the score, and reject regressions in every category
        # covered by the existing product policy, not just a global average.
        preserved_policy = {"DATE", "AGE", "SEX", "GENDER", "TITLE", "date", "date_time", "time", "date_of_birth"}
        critical = [label for label in before["byLabel"] if label not in preserved_policy]
        for label in critical:
            if label in before["byLabel"]:
                improved &= after["byLabel"][label]["removed"] >= before["byLabel"][label]["removed"]
        comparisons[name] = dict(before=before, after=after, promotion_gate=bool(improved))
    result = dict(model_sha256=trained_hash, model_bytes=trained.stat().st_size,
                  best_checkpoint=selection, benchmarks=comparisons,
                  promoted=all(c["promotion_gate"] for c in comparisons.values()),
                  interpretation="Removal coverage, not an anonymity guarantee. All original labels and failures retained.")
    (RUN / "comparison.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))
    lines = ["# Resultado del postentrenamiento", "", "| Corpus | Antes | Después | Residuos antes → después |", "|---|---:|---:|---:|"]
    for name, c in comparisons.items():
        a,b=c["before"],c["after"]
        lines.append(f"| {name} | {a['removalCoverage']:.1%} | {b['removalCoverage']:.1%} | {a['visible']} → {b['visible']} |")
    lines += ["", "Métrica: desaparición de valores anotados del texto protegido. Muestras pequeñas; no garantiza anonimato.",
              "La evaluación usa el motor Rust real y los mismos 52 documentos del baseline.", "",
              "Cumple el criterio de mejora para incorporarlo a la app." if result["promoted"] else "No cumple el criterio de mejora. Se conservan pesos y resultados; el modelo anterior sigue activo."]
    (RUN / "RESULTADO.md").write_text("\n".join(lines)+"\n")
    if not result["promoted"]:
        return dict(phase="trained_evaluated_not_promoted", comparison=str(RUN/"RESULTADO.md"))
    manifest_path = ROOT / "src-tauri/model-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["belegal-pii-v1"] = dict(url="", size=trained.stat().st_size, sha256=trained_hash,
        license_source="https://huggingface.co/Qwen/Qwen3-1.7B/blob/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/LICENSE",
        provenance="Local QLoRA. Training/evaluation records: artifacts/posttraining")
    manifest_path.write_text(json.dumps(manifest, indent=2)+"\n")
    stage("catalog-tests", ["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "models::portability_tests"])
    stage("frontend-check", ["pnpm", "check"])
    stage("native-check", ["cargo", "clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"])
    stage("build-app", [sys.executable, "scripts/build-local-demo.py", "--no-open"])
    models = Path.home()/"Library/Application Support/com.bluedoor.desktop/models"
    destination = models/trained.name
    if destination.exists():
        assert sha(destination) == trained_hash, "A different trained model is installed; preserve it before replacement"
    else:
        temporary = models/(trained.name+".installing")
        shutil.copy2(trained, temporary)
        assert sha(temporary) == trained_hash
        temporary.replace(destination)
    built = ROOT/"src-tauri/target/debug/bundle/macos/BlueDoor.app"
    installed = Path("/Applications/BlueDoor.app")
    stamp = time.strftime("%Y%m%d-%H%M%S")
    staging = Path(f"/Applications/BlueDoor-posttraining-{stamp}.app")
    backup = Path.home()/f"Downloads/BlueDoor/BlueDoor antes del postentrenamiento {stamp}.app"
    assert not staging.exists() and not backup.exists()
    subprocess.run(["ditto", str(built), str(staging)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(staging)], check=True)
    backup.parent.mkdir(parents=True, exist_ok=True)
    installed.rename(backup)
    try: staging.rename(installed)
    except BaseException:
        backup.rename(installed)
        raise
    # Keep the user's in-memory document session. The new selectable model
    # becomes visible the next time the installed app starts.
    return dict(phase="trained_evaluated_installed", model=str(destination), model_sha256=trained_hash,
                comparison=str(RUN/"RESULTADO.md"), app=str(installed), app_restart_required=True,
                default_model_changed=False, backup=str(backup))

try:
    result = finish()
    result["updated"] = time.time()
    (RUN / "pipeline-status.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2), flush=True)
except BaseException as error:
    (RUN / "pipeline-status.json").write_text(json.dumps(dict(phase="failed", error=repr(error), updated=time.time()), indent=2))
    raise
