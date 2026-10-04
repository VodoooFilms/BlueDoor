#!/usr/bin/env python3
"""Score observed output against external gold values, retaining all misses.

This deliberately reports the full dataset taxonomy. Dates/titles that BeLegal
intentionally preserves remain visible failures under the dataset's policy.
"""
import collections
import json
from pathlib import Path
import re
import sys

path = Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/benchmark/baseline-results.json")
results = json.loads(path.read_text())
counts = collections.defaultdict(lambda: collections.Counter(total=0, removed=0))
documents = []
for result in results:
    row = result["input"]
    if row["uid"] == "paypal-local":
        continue
    output = result.get("sanitized", row["source_text"])
    missed = []
    for span in row["privacy_mask"]:
        assert row["source_text"][span["start"]:span["end"]] == span["value"], "Gold offsets are invalid"
        parts = re.split(r"\s+", span["value"])
        pattern = r"\s+".join(re.escape(part) for part in parts)
        if span["value"][0].isalnum():
            pattern = r"(?<!\w)" + pattern
        if span["value"][-1].isalnum():
            pattern += r"(?!\w)"
        visible = bool(re.search(pattern, output, re.IGNORECASE))
        counts[span["label"]]["total"] += 1
        counts[span["label"]]["removed"] += not visible
        if visible:
            missed.append(span)
    documents.append({"uid": row["uid"], "language": row["language"], "missed": missed, "error": result.get("error"), "complete": result.get("complete", False), "seconds": result["seconds"]})
total = sum(c["total"] for c in counts.values())
removed = sum(c["removed"] for c in counts.values())
summary = {
    "method": "Case-insensitive whole-value residual search in actual sanitized output; all gold labels included. A repeated gold value is considered protected only if absent everywhere. This is value removal coverage, not exact span F1 or proof of anonymity.",
    "documents": len(documents), "goldAnnotations": total, "removed": removed,
    "visible": total - removed, "removalCoverage": removed / total,
    "documentsWithoutGoldResiduals": sum(not d["missed"] and d["complete"] and not d["error"] for d in documents),
    "incompleteDocuments": sum(not d["complete"] for d in documents),
    "errors": sum(bool(d["error"]) for d in documents),
    "byLabel": dict(counts), "details": documents,
}
destination = path.with_name(path.stem.removesuffix("-results") + "-score.json")
destination.write_text(json.dumps(summary, ensure_ascii=False, indent=2))
print(json.dumps({k: v for k, v in summary.items() if k not in ["details", "method"]}, ensure_ascii=False, indent=2))
