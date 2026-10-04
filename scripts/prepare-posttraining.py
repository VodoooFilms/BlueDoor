#!/usr/bin/env python3
"""Build a reproducible, source-grouped SFT corpus; never read private documents."""
import collections
import hashlib
import json
import re
import zipfile
from pathlib import Path

import pyarrow.parquet as pq
from transformers import AutoTokenizer

ROOT = Path(__file__).resolve().parent.parent
RUN = ROOT / "artifacts/posttraining"
OUT = RUN / "data"
OUT.mkdir(exist_ok=True)
SEED = "belegal-pii-v1-20261003"
MAX_TOKENS = 2048
template = (RUN / "prompt-template.txt").read_text()
assert template.count("{{DOCUMENT}}") == 1
tokenizer = AutoTokenizer.from_pretrained(RUN / "base-hf", local_files_only=True)
stats = collections.Counter()
labels = collections.Counter()
datasets = collections.Counter()
rows = []

# Product policy: retain dates, amounts, generic titles, ages and gender.
# The existing external benchmark still scores ALL its labels, unchanged.
POLICY = {
    "GIVENNAME": "PERSON", "SURNAME": "PERSON", "EMAIL": "EMAIL",
    "TELEPHONENUM": "PHONE", "STREET": "ADDRESS", "CITY": "ADDRESS",
    "BUILDINGNUM": "ADDRESS", "ZIPCODE": "ADDRESS",
    "DRIVERLICENSENUM": "ID", "SOCIALNUM": "ID", "TAXNUM": "ID",
    "IDCARDNUM": "ID", "CREDITCARDNUMBER": "ID", "PASSPORTNUM": "ID",
    "name": "PERSON", "first_name": "PERSON", "last_name": "PERSON",
    "company": "COMPANY", "email": "EMAIL", "phone_number": "PHONE",
    "street_address": "ADDRESS", "local_latlng": "ADDRESS",
    **{k: "ID" for k in ["account_pin", "bank_routing_number", "bban", "credit_card_number",
       "credit_card_security_code", "customer_id", "driver_license_number", "employee_id",
       "iban", "passport_number", "ssn", "swift_bic_code"]},
    **{k: "OTHER" for k in ["api_key", "ipv4", "ipv6", "password", "user_name"]},
    **{k: "PERSON" for k in ["NOMBRE_SUJETO_ASISTENCIA", "NOMBRE_PERSONAL_SANITARIO"]},
    **{k: "ID" for k in ["ID_SUJETO_ASISTENCIA", "ID_TITULACION_PERSONAL_SANITARIO",
       "ID_ASEGURAMIENTO", "ID_CONTACTO_ASISTENCIAL"]},
    **{k: "ADDRESS" for k in ["TERRITORIO", "CALLE", "PAIS"]},
    **{k: "COMPANY" for k in ["HOSPITAL", "INSTITUCION", "CENTRO_SALUD"]},
    "CORREO_ELECTRONICO": "EMAIL", "NUMERO_TELEFONO": "PHONE", "NUMERO_FAX": "PHONE",
    "OTROS_SUJETO_ASISTENCIA": "OTHER",
    "PERSON": "PERSON", "ORG": "COMPANY", "PERSON_EMAIL": "EMAIL",
    "CODE_PHONE": "PHONE", "CODE_URL": "OTHER", "CODE": "ID",
    **{k: "ADDRESS" for k in ["LOC_COUNTRY", "LOC_CITY", "LOC_OTHER", "LOC_STREET", "LOC_HOUSENUMBER", "LOC_ZIP"]},
}

def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()

def fingerprint(text, spans):
    for span in sorted(spans, key=lambda s: s["start"], reverse=True):
        text = text[:span["start"]] + " <" + span["label"] + "> " + text[span["end"]:]
    return digest(re.sub(r"\d+", "#", " ".join(text.casefold().split())))

def neutralize(text):
    lines = text.splitlines(keepends=True)
    for i, line in enumerate(lines):
        body = line.rstrip("\n")
        tag = body.rstrip("\r").strip()
        if tag.casefold() in ["system", "user", "assistant", "tool", "/no_think"]:
            lines[i] = f"[role:{tag}]" + ("\n" if line.endswith("\n") else "")
    return "".join(lines).replace("<|", "< | ").replace("|>", " | >")

def add(source, uid, language, text, spans, group=None):
    stats[f"input:{source}"] += 1
    spans = sorted(spans, key=lambda s: (s["start"], s["end"]))
    for s in spans:
        if not 0 <= s["start"] < s["end"] <= len(text) or text[s["start"]:s["end"]] != s["value"]:
            stats[f"rejected_invalid_offsets:{source}"] += 1
            return
    fp = fingerprint(text, spans)
    if fp in frozen_templates or digest(text) in frozen_texts:
        stats["excluded_frozen_test_overlap"] += 1
        return
    key = group or fp
    split = "valid" if int(digest(SEED + key)[:8], 16) % 20 == 0 else "train"
    rows.append(dict(source=source, uid=str(uid), language=language, text=text,
                     spans=spans, group=key, fingerprint=fp, split=split))

frozen_texts, frozen_templates = set(), set()
frozen_checksums = {}
for name in ["openpii-validation.jsonl", "gretel-validation.jsonl"]:
    path = ROOT / "artifacts/benchmark" / name
    frozen_checksums[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    for line in path.read_text().splitlines():
        row = json.loads(line)
        frozen_texts.add(digest(row["source_text"]))
        frozen_templates.add(fingerprint(row["source_text"], row["privacy_mask"]))

selection = json.loads((RUN / "openpii-selection.json").read_text())
assert selection["source_split"] == "train" and min(selection["counts"].values()) == 6000
for line in (RUN / "openpii-train-selected.jsonl").read_text().splitlines():
    r = json.loads(line)
    add("openpii", r["uid"], r["language"], r["source_text"], r["privacy_mask"])

for language in ["Spanish", "English"]:
    path = RUN / "gretel/data" / f"{language}_train-00000-of-00001.parquet"
    candidates = pq.read_table(path).to_pylist()
    # Deterministic domain-balanced sample; all selected sources are official train.
    candidates.sort(key=lambda r: digest(r["generated_text"]))
    selected = 0
    for i, r in enumerate(candidates):
        if r["quality_score"] < 85 or r["conformance_score"] < 85 or r["groundedness_score"] < 80:
            stats[f"excluded_quality:gretel-{language}"] += 1
            continue
        if selected >= 4000:
            break
        spans = json.loads(r["pii_spans"]) if isinstance(r["pii_spans"], str) else r["pii_spans"]
        text = r["generated_text"]
        for s in spans: s["value"] = text[s["start"]:s["end"]]
        add("gretel", f"{language}:{i}", {"Spanish":"es", "English":"en"}[language], text, spans)
        selected += 1

with zipfile.ZipFile(ROOT / "artifacts/datasets/meddocan.zip") as archive:
    for name in sorted(archive.namelist()):
        if "/train/brat/" not in name or not name.endswith(".ann"): continue
        text = archive.read(name[:-4] + ".txt").decode("utf-8-sig")
        spans = []
        for line in archive.read(name).decode("utf-8-sig").splitlines():
            if not line.startswith("T"): continue
            _, descriptor, value = line.split("\t", 2)
            label, start, end = descriptor.split()
            spans.append(dict(label=label, start=int(start), end=int(end), value=value))
        add("meddocan", name, "es", text, spans)

with zipfile.ZipFile(ROOT / "artifacts/datasets/DialogPII.zip") as archive:
    for name in sorted(archive.namelist()):
        if not name.endswith(".json") or "/transcripts/" not in name: continue
        if not any(f"/{lang}/" in name for lang in ["SP", "EN"]): continue
        data = json.loads(archive.read(name))
        for dialog in data["dialogs"]:
            text, spans = "", []
            for turn in dialog["dialog"]["turns"]:
                prefix = f"{turn['speaker']}: "
                offset = len(text) + len(prefix)
                for s in turn["annotations"]:
                    spans.append(dict(label=s["type"], start=offset+s["start"], end=offset+s["end"], value=s["text"]))
                text += prefix + turn["text"] + "\n"
            add("dialogpii", f"{name}:{dialog['chat_number']}", "es" if "/SP/" in name else "en",
                text, spans, group=f"dialogpii:{data['scenario']}:{dialog['chat_number']}")

# Union source-dialog groups and duplicate template groups before splitting.
# A translated dialogue cannot enter both partitions through another template.
parent = {}
def find(key):
    parent.setdefault(key, key)
    if parent[key] != key: parent[key] = find(parent[key])
    return parent[key]
for row in rows:
    a, b = find("group:" + row["group"]), find("template:" + row["fingerprint"])
    parent[max(a, b)] = min(a, b)
for row in rows:
    row["group"] = find("group:" + row["group"])
    row["split"] = "valid" if int(digest(SEED + row["group"])[:8], 16) % 20 == 0 else "train"
# Exact duplicates and template variants stay together across all corpora.
seen = set()
files = {s: (OUT / f"{s}.jsonl").open("w") for s in ["train", "valid"]}
try:
    for row in rows:
        text = row["text"]
        if digest(text) in seen:
            stats["excluded_exact_duplicate"] += 1
            continue
        seen.add(digest(text))
        start, width = 0, 2000
        while start < len(text):
            end = min(len(text), start + width)
            # Never cut an annotated entity at a training fragment boundary.
            for span in row["spans"]:
                if span["start"] < end < span["end"]:
                    end = span["start"] if span["start"] > start else span["end"]
                    break
            document = text[start:end]
            targets, used = [], set()
            for s in row["spans"]:
                kind = POLICY.get(s["label"])
                if kind and start <= s["start"] and s["end"] <= end and 2 <= len(s["value"].encode()) <= 500:
                    key = (kind, s["value"])
                    if key not in used:
                        targets.append(dict(kind=kind, text=s["value"]))
                        used.add(key)
            prompt = template.replace("{{DOCUMENT}}", neutralize(document))
            completion = json.dumps({"entities": targets}, ensure_ascii=False, separators=(",", ":"))
            prompt_tokens = tokenizer.encode(prompt, add_special_tokens=False)
            answer_tokens = tokenizer.encode(completion, add_special_tokens=False) + [tokenizer.eos_token_id]
            if len(prompt_tokens) + len(answer_tokens) > MAX_TOKENS:
                if width <= 125: raise ValueError(f"Cannot fit source entity without truncating: {row['uid']}")
                width //= 2
                continue
            record = dict(prompt=prompt, completion=completion, source=row["source"], uid=row["uid"],
                language=row["language"], group=row["group"], start=start, end=end,
                token_count=len(prompt_tokens)+len(answer_tokens), prompt_tokens=len(prompt_tokens))
            files[row["split"]].write(json.dumps(record, ensure_ascii=False) + "\n")
            stats[row["split"]] += 1
            datasets[f"{row['split']}:{row['source']}:{row['language']}"] += 1
            labels.update(t["kind"] for t in targets)
            if not targets: stats[f"negative:{row['split']}"] += 1
            start, width = end, 2000
finally:
    for file in files.values(): file.close()

assert stats["train"] > 5000 and stats["valid"] > 200
manifest = dict(seed=SEED, max_tokens=MAX_TOKENS, counts=dict(stats), by_source=dict(datasets),
    gretel_minimum_scores=dict(quality=85, conformance=85, groundedness=80),
    target_kinds=dict(labels), policy=POLICY, excluded_policy=["dates", "amounts", "age", "gender", "generic titles"],
    prompt_sha256=digest(template), frozen_test_sha256=frozen_checksums, openpii_source=selection,
    train_sha256=hashlib.sha256((OUT/"train.jsonl").read_bytes()).hexdigest(),
    valid_sha256=hashlib.sha256((OUT/"valid.jsonl").read_bytes()).hexdigest())
(OUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2))
print(json.dumps({k:manifest[k] for k in ["counts", "by_source", "target_kinds"]}, indent=2))
