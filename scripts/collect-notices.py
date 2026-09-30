#!/usr/bin/env python3
"""Gather license evidence for the macOS runtime and conservative build closure."""
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "src-tauri/resources/runtime"
LICENSES = OUT / "licenses"
TARGET = "aarch64-apple-darwin"

LICENSE_FALLBACKS = {
    "alloc-stdlib": "alloc-stdlib",
    "block2": "objc2",
    "defmt-parser": "defmt",
    "dispatch2": "objc2",
    "encoding-index-japanese": "encoding-index",
    "encoding-index-korean": "encoding-index",
    "encoding-index-simpchinese": "encoding-index",
    "encoding-index-singlebyte": "encoding-index",
    "encoding-index-tradchinese": "encoding-index",
    "encoding_index_tests": "encoding-index",
    "genpdf": "genpdf",
    "objc2": "objc2",
    "objc2-app-kit": "objc2",
    "objc2-core-foundation": "objc2",
    "objc2-core-graphics": "objc2",
    "objc2-encode": "objc2",
    "objc2-exception-helper": "objc2",
    "objc2-foundation": "objc2",
    "objc2-io-surface": "objc2",
    "objc2-web-kit": "objc2",
    "selectors": "selectors",
    "unic-char-property": "unic",
    "unic-char-range": "unic",
    "unic-common": "unic",
    "unic-ucd-ident": "unic",
    "unic-ucd-version": "unic",
}


def license_files(directory, recursive=False):
    files = directory.rglob("*") if recursive else directory.iterdir()
    return [
        path for path in files
        if path.is_file()
        and path.name.upper().startswith(
            ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
        )
        and path.stat().st_size < 1_000_000
    ]


def cargo_closure(metadata):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]

    def reachable(kinds):
        seen, pending = set(), [root]
        while pending:
            package_id = pending.pop()
            if package_id in seen:
                continue
            seen.add(package_id)
            for dependency in nodes[package_id]["deps"]:
                dependency_kinds = {item["kind"] for item in dependency["dep_kinds"]}
                if dependency_kinds & kinds:
                    pending.append(dependency["pkg"])
        return seen

    return reachable({None}), reachable({None, "build"})


def npm_packages():
    npm_cmd = "pnpm.cmd" if os.name == "nt" else "pnpm"
    listing = json.loads(subprocess.check_output(
        [npm_cmd, "list", "--json", "--prod", "--depth", "Infinity"],
        cwd=ROOT,
    ))[0]
    runtime = {}

    def walk(dependencies):
        for name, package in (dependencies or {}).items():
            runtime[(name, package["version"])] = Path(package["path"])
            walk(package.get("dependencies"))

    walk(listing.get("dependencies"))
    installed = {}
    for pattern in (
        "*/node_modules/*/package.json",
        "*/node_modules/@*/*/package.json",
    ):
        for package_json in (ROOT / "node_modules/.pnpm").glob(pattern):
            data = json.loads(package_json.read_text(encoding="utf-8"))
            key = (
                data.get("name", package_json.parent.name),
                data.get("version", "unknown"),
            )
            installed[key] = package_json.parent
    return runtime, installed


def collect_js(name, version, directory, scope, inventory):
    data = json.loads((directory / "package.json").read_text(encoding="utf-8"))
    files = license_files(directory)
    dest = LICENSES / "js" / f'{name.replace("/", "_")}-{version}'
    for source in files:
        dest.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest / source.name)
    author = data.get("author")
    inventory.append({
        "ecosystem": "npm",
        "scope": scope,
        "name": name,
        "version": version,
        "license": data.get("license"),
        "source": data.get("repository") or data.get("homepage"),
        "authors": [author] if author else [],
        "license_files": [source.name for source in files],
    })


def collect():
    OUT.mkdir(parents=True, exist_ok=True)
    if LICENSES.exists():
        shutil.rmtree(LICENSES)
    LICENSES.mkdir()
    inventory = []
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--filter-platform", TARGET,
        "--manifest-path", str(ROOT / "src-tauri/Cargo.toml"),
    ], cwd=ROOT))
    normal, conservative = cargo_closure(metadata)
    selected = [item for item in metadata["packages"] if item["id"] in conservative]
    for package in sorted(selected, key=lambda item: (item["name"], item["version"])):
        directory = Path(package["manifest_path"]).parent
        dest = LICENSES / "rust" / f'{package["name"]}-{package["version"]}'
        files = license_files(
            directory,
            package["name"] in {"whisper-rs-sys"},
        )
        for source in files:
            target = dest / source.relative_to(directory)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
        copied_files = [str(source.relative_to(directory)) for source in files]
        if not copied_files and package["name"] in LICENSE_FALLBACKS:
            fallback = (
                ROOT / "licenses/dependencies" / LICENSE_FALLBACKS[package["name"]]
            )
            for source in sorted(fallback.iterdir()):
                if source.is_file():
                    target = dest / "upstream" / source.name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, target)
                    copied_files.append(str(target.relative_to(dest)))
        inventory.append({
            "ecosystem": "rust",
            "scope": "runtime-linked" if package["id"] in normal else "build-only",
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license"),
            "source": package.get("repository") or package.get("source"),
            "authors": package.get("authors", []),
            "license_files": copied_files,
        })

    runtime_js, installed_js = npm_packages()
    bundled_frameworks = {"svelte", "esm-env"}
    for (name, version), directory in sorted(installed_js.items()):
        if (name, version) in runtime_js or name in bundled_frameworks:
            scope = "runtime-bundled"
        else:
            scope = "build-only"
        collect_js(name, version, directory, scope, inventory)

    for source in (ROOT / "licenses/models").glob("*.txt"):
        shutil.copyfile(source, LICENSES / source.name)
    (LICENSES / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
    summary = {
        "target": TARGET,
        "total": len(inventory),
        "runtime": sum(item["scope"].startswith("runtime") for item in inventory),
        "build_only": sum(item["scope"] == "build-only" for item in inventory),
        "missing_license_expression": sum(not item["license"] for item in inventory),
        "runtime_missing_license_file": sum(
            item["scope"].startswith("runtime")
            and item["name"] != "belegal-local-demo"
            and not item["license_files"]
            for item in inventory
        ),
    }
    (LICENSES / "inventory-summary.json").write_text(
        json.dumps(summary, indent=2) + "\n"
    )
    shutil.copyfile(ROOT / "THIRD_PARTY_NOTICES.md", OUT / "THIRD_PARTY_NOTICES.md")
    shutil.copyfile(ROOT / "LICENSE", OUT / "Belegal _ Local demo-LICENSE.txt")
    shutil.copyfile(ROOT / "scripts/build-ffmpeg.py", OUT / "ffmpeg-build.py")
    shutil.copyfile(
        ROOT / ".build-cache/ffmpeg-8.1.2.tar.xz", OUT / "ffmpeg-source.tar.xz"
    )
    print(
        f"Collected license evidence for {summary['runtime']} runtime and "
        f"{summary['build_only']} build-only packages"
    )


if __name__ == "__main__":
    collect()
