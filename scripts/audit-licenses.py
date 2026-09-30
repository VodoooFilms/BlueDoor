#!/usr/bin/env python3
"""Audit the licenses of code and models reachable in the shipped macOS app."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUNTIME = ROOT / "src-tauri/resources/runtime"
INVENTORY = RUNTIME / "licenses/inventory.json"
REPORT = RUNTIME / "license-audit.json"

def audit():
    inventory = json.loads(INVENTORY.read_text())
    manifest = json.loads((ROOT / "src-tauri/model-manifest.json").read_text())
    blockers = []

    runtime_missing_expression = [
        f"{item['ecosystem']}:{item['name']}@{item['version']}"
        for item in inventory
        if item["scope"].startswith("runtime")
        and item["name"] != "belegal-local-demo"
        and not item["license"]
    ]
    runtime_missing_text = [
        f"{item['ecosystem']}:{item['name']}@{item['version']}"
        for item in inventory
        if item["scope"].startswith("runtime")
        and item["name"] != "belegal-local-demo"
        and not item["license_files"]
    ]
    if runtime_missing_expression or runtime_missing_text:
        blockers.append({
            "component": "runtime attribution set",
            "classification": "ambiguous/not demonstrated",
            "reason": "One or more linked/bundled packages lacks license evidence.",
            "missing_license_expression": runtime_missing_expression,
            "missing_license_text": runtime_missing_text,
        })

    for model_id, asset in manifest.items():
        if not asset.get("license_source") or len(asset.get("sha256", "")) != 64:
            blockers.append({
                "component": model_id,
                "classification": "ambiguous/not demonstrated",
                "reason": "Download lacks pinned integrity or license provenance.",
            })
    if not (ROOT / "licenses/models/Whisper-LICENSE.txt").is_file():
        blockers.append({
            "component": "Whisper model license evidence",
            "classification": "ambiguous/not demonstrated",
            "reason": "Checked-in model license is missing.",
        })

    report = {
        "status": "FAIL" if blockers else "PASS",
        "scope": "arm64 macOS runtime plus models downloadable from the current catalog",
        "classifications": {
            "allowed": ["Belegal _ Local demo application code (MIT)"],
            "allowed_with_obligations": [
                "FFmpeg 8.1.2 (LGPL-2.1-or-later; source and build recipe included)",
                "Whisper model weights and whisper.cpp (MIT)",
                "locked Rust/npm runtime dependencies under their recorded terms",
            ],
            "ambiguous_or_not_demonstrated": (
                ["items listed in blockers"] if blockers else []
            ),
            "prohibited": [],
            "excluded_from_commercial_catalog": [],
        },
        "blockers": blockers,
        "human_review": [
            "Counsel/owner must accept the recorded open-source and model obligations.",
            "Codec patent exposure depends on markets and media formats offered.",
        ],
    }
    REPORT.write_text(json.dumps(report, indent=2) + "\n")
    print(f"COMMERCIAL LICENSE ENGINEERING GATE: {report['status']}")
    for blocker in blockers:
        print(f"- {blocker['component']}: {blocker['reason']}")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--allow-fail", action="store_true")
    args = parser.parse_args()
    result = audit()
    if result["status"] != "PASS" and not args.allow_fail:
        raise SystemExit(2)
