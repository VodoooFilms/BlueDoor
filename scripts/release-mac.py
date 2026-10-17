#!/usr/bin/env python3
"""Canonical macOS release: prepare, sign, notarize, staple and verify."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "src-tauri/target/portable"
REL = TARGET / "aarch64-apple-darwin/release"
APP = REL / "bundle/macos/BlueDoor.app"
LOGS = REL / "bundle/notarization"
DEVELOPMENT = os.environ.get("BLUEDOOR_RELEASE_MODE") == "development"


def run(args, **kwargs):
    subprocess.run([str(arg) for arg in args], cwd=ROOT, check=True, **kwargs)


def capture(args, **kwargs):
    return subprocess.run(
        [str(arg) for arg in args], cwd=ROOT, check=False,
        capture_output=True, text=True, **kwargs
    )


def load_verifier():
    spec = importlib.util.spec_from_file_location(
        "verify_bundle", ROOT / "scripts/verify-bundle.py"
    )
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    return verifier


def prepare_bundle():
    run([sys.executable, ROOT / "scripts/build-ffmpeg.py"])
    run([sys.executable, ROOT / "scripts/collect-notices.py"])
    run([sys.executable, ROOT / "scripts/audit-licenses.py", "--allow-fail"])
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(TARGET)
    env["MACOSX_DEPLOYMENT_TARGET"] = "13.4"
    env["CMAKE_OSX_DEPLOYMENT_TARGET"] = "13.4"
    for key in [
        "APPLE_SIGNING_IDENTITY", "APPLE_CERTIFICATE",
        "APPLE_CERTIFICATE_PASSWORD", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH",
        "LIBRARY_PATH", "PKG_CONFIG_PATH",
    ]:
        env.pop(key, None)
    run([
        "pnpm", "tauri", "build", "--target", "aarch64-apple-darwin",
        "--bundles", "app", "--no-sign", "--", "--locked",
    ], env=env)
    if not APP.is_dir():
        raise SystemExit(f"Missing exact build output: {APP}")

    run([sys.executable, ROOT / "scripts/package-local-ai.py", APP, REL])
    verifier = load_verifier()
    binary = APP / "Contents/MacOS/bluedoor"
    for native in verifier.macho_files(APP):
        for rpath in verifier.rpaths(native):
            if rpath.startswith("/"):
                run(["install_name_tool", "-delete_rpath", rpath, native])
    run([sys.executable, ROOT / "scripts/verify-bundle.py", APP])
    run([sys.executable, ROOT / "scripts/verify-icons.py", APP])
    return verifier, binary


def developer_identities():
    result = capture(["security", "find-identity", "-v", "-p", "codesigning"])
    identities = []
    for line in (result.stdout + result.stderr).splitlines():
        match = re.search(
            r"\)\s+([0-9A-F]{40})\s+\"(Developer ID Application: .+)\"", line
        )
        if match:
            identities.append((match.group(1), match.group(2)))
    return identities


def select_identity():
    identities = developer_identities()
    requested = os.environ.get("BLUEDOOR_SIGNING_IDENTITY", "").strip()
    if requested:
        matches = [item for item in identities if requested in item]
        if len(matches) != 1:
            return None, (
                "BLUEDOOR_SIGNING_IDENTITY does not select exactly one valid "
                "Developer ID Application identity in this login keychain"
            )
        return matches[0][1], None
    if len(identities) == 1:
        return identities[0][1], None
    if not identities:
        return None, "a valid Developer ID Application certificate and private key"
    names = ", ".join(name for _, name in identities)
    return None, (
        "BLUEDOOR_SIGNING_IDENTITY selecting one available identity: " + names
    )


def notary_auth():
    profile = os.environ.get("BLUEDOOR_NOTARY_PROFILE", "").strip()
    if profile:
        return ["--keychain-profile", profile], None
    values = {
        "BLUEDOOR_NOTARY_KEY_ID": os.environ.get("BLUEDOOR_NOTARY_KEY_ID", "").strip(),
        "BLUEDOOR_NOTARY_ISSUER_ID": os.environ.get("BLUEDOOR_NOTARY_ISSUER_ID", "").strip(),
        "BLUEDOOR_NOTARY_KEY_PATH": os.environ.get("BLUEDOOR_NOTARY_KEY_PATH", "").strip(),
    }
    if all(values.values()):
        key_path = Path(values["BLUEDOOR_NOTARY_KEY_PATH"]).expanduser()
        if not key_path.is_file():
            return None, (
                "BLUEDOOR_NOTARY_KEY_PATH pointing to an existing private API key"
            )
        return [
            "--key", key_path,
            "--key-id", values["BLUEDOOR_NOTARY_KEY_ID"],
            "--issuer", values["BLUEDOOR_NOTARY_ISSUER_ID"],
        ], None
    return None, (
        "BLUEDOOR_NOTARY_PROFILE naming a notarytool Keychain profile, or all of "
        "BLUEDOOR_NOTARY_KEY_ID / BLUEDOOR_NOTARY_ISSUER_ID / "
        "BLUEDOOR_NOTARY_KEY_PATH"
    )


def signing_preflight():
    identity, identity_error = select_identity()
    auth, auth_error = notary_auth()
    signing_blockers = [error for error in (identity_error, auth_error) if error]
    audit = json.loads((ROOT / "src-tauri/resources/runtime/license-audit.json").read_text())
    commercial_blockers = [item["reason"] for item in audit["blockers"]]
    if signing_blockers or commercial_blockers:
        print(f"READY FOR SIGNING: {'NO' if commercial_blockers else 'YES'}")
        if commercial_blockers:
            print("COMMERCIAL RELEASE BLOCKED BY: " + "; ".join(commercial_blockers))
        if signing_blockers:
            print("SIGNING BLOCKED BY: " + "; ".join(signing_blockers))
        raise SystemExit(2)
    return identity, auth


def sign_bundle(verifier, binary, identity, *, hardened_runtime):
    seen = set()
    nested = []
    for native in verifier.macho_files(APP):
        resolved = native.resolve()
        if resolved == binary.resolve() or resolved in seen:
            continue
        seen.add(resolved)
        nested.append(resolved)
    signing_args = ["--force"]
    if hardened_runtime:
        signing_args.extend(["--options", "runtime"])
    if identity != "-":
        signing_args.append("--timestamp")
    signing_args.extend(["--sign", identity])
    for native in sorted(nested):
        run(["codesign", *signing_args, native])
    run([
        "codesign", *signing_args, "--entitlements",
        ROOT / "src-tauri/entitlements.mac.plist", APP,
    ])
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", APP])
    details = capture(["codesign", "-d", "--verbose=4", APP])
    detail_text = details.stdout + details.stderr
    if hardened_runtime and "runtime" not in detail_text:
        raise SystemExit("Signed app does not have the hardened runtime flag")
    if identity != "-" and "Authority=Developer ID Application:" not in detail_text:
        raise SystemExit(
            "Signed app does not report a Developer ID Application authority"
        )


def notary_submit(path: Path, label: str, auth):
    LOGS.mkdir(parents=True, exist_ok=True)
    result = capture([
        "xcrun", "notarytool", "submit", path, "--wait",
        "--output-format", "json", *auth,
    ])
    (LOGS / f"{label}-submit.json").write_text(result.stdout + result.stderr)
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        payload = {}
    submission_id = payload.get("id")
    log_path = LOGS / f"{label}-log.json"
    if submission_id:
        log_result = capture([
            "xcrun", "notarytool", "log", submission_id, *auth
        ])
        log_path.write_text(log_result.stdout + log_result.stderr)
        if log_result.returncode != 0:
            raise SystemExit(
                f"Could not retrieve Apple's notarization log for {label}; inspect {log_path}"
            )
    if result.returncode != 0 or payload.get("status") != "Accepted" or not submission_id:
        evidence = log_path if submission_id else LOGS / f"{label}-submit.json"
        raise SystemExit(
            f"Apple notarization rejected {label}; inspect {evidence}"
        )
    try:
        log_payload = json.loads(log_path.read_text()) if submission_id else {}
    except json.JSONDecodeError:
        log_payload = {}
    if log_payload.get("issues"):
        raise SystemExit(
            f"Apple accepted {label} with reported issues; inspect {log_path}"
        )
    print(f"Notarization accepted for {label}: {submission_id}")


def create_dmg(output: Path):
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()
    with tempfile.TemporaryDirectory(prefix="bluedoor-dmg-") as stage:
        stage_path = Path(stage)
        run(["ditto", APP, stage_path / "BlueDoor.app"])
        (stage_path / "Applications").symlink_to("/Applications")
        run([
            "hdiutil", "create", "-volname", "BlueDoor",
            "-srcfolder", stage, "-ov", "-format", "UDZO", output,
        ])


def write_checksum(output: Path):
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_suffix(".dmg.sha256").write_text(f"{digest}  {output.name}\n")
    return digest


def development_release(verifier, binary, version):
    # Ad-hoc signatures have no Team ID. Enabling library validation here makes
    # macOS reject the bundled GGML libraries even though they are also signed.
    # Hardened runtime remains mandatory for the Developer ID release path.
    sign_bundle(verifier, binary, "-", hardened_runtime=False)
    run([sys.executable, ROOT / "scripts/verify-bundle.py", APP])
    output = (
        REL / "bundle/dmg" / f"BlueDoor_{version}_development_aarch64.dmg"
    )
    create_dmg(output)
    run(["hdiutil", "verify", output])
    digest = write_checksum(output)
    print(
        f"DEVELOPMENT ARTIFACT ONLY: {output}\nSHA-256: {digest}\n"
        "Ad-hoc signed; not notarized and not approved for commercial distribution."
    )


def commercial_release(verifier, binary, version):
    identity, auth = signing_preflight()
    sign_bundle(verifier, binary, identity, hardened_runtime=True)
    run([sys.executable, ROOT / "scripts/verify-bundle.py", APP])
    run([sys.executable, ROOT / "scripts/verify-icons.py", APP])

    with tempfile.TemporaryDirectory(prefix="bluedoor-notary-") as temporary:
        archive = Path(temporary) / "BlueDoor.zip"
        run(["ditto", "-c", "-k", "--keepParent", APP, archive])
        notary_submit(archive, "app", auth)
    run(["xcrun", "stapler", "staple", APP])
    run(["xcrun", "stapler", "validate", APP])

    output = REL / "bundle/dmg" / f"BlueDoor_{version}_aarch64.dmg"
    create_dmg(output)
    run(["codesign", "--force", "--timestamp", "--sign", identity, output])
    notary_submit(output, "dmg", auth)
    run(["xcrun", "stapler", "staple", output])
    run(["xcrun", "stapler", "validate", output])
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", APP])
    run(["spctl", "--assess", "--type", "execute", "--verbose=4", APP])
    run([
        "spctl", "--assess", "--type", "open", "--context",
        "context:primary-signature", "--verbose=4", output,
    ])
    run(["hdiutil", "verify", output])
    digest = write_checksum(output)
    print(
        f"COMMERCIAL RELEASE CANDIDATE: {output}\nSHA-256: {digest}\n"
        "Developer ID, hardened runtime, notarization, stapling and local "
        "Gatekeeper checks PASS."
    )


def main():
    if len(sys.argv) != 1:
        raise SystemExit("Use pnpm release:mac or pnpm release:mac:dev")
    version = json.loads((ROOT / "package.json").read_text())["version"]
    if not DEVELOPMENT:
        production = REL / "bundle/dmg" / f"BlueDoor_{version}_aarch64.dmg"
        for stale in (production, production.with_suffix(".dmg.sha256")):
            if stale.exists():
                stale.unlink()
    verifier, binary = prepare_bundle()
    if DEVELOPMENT:
        development_release(verifier, binary, version)
    else:
        commercial_release(verifier, binary, version)


if __name__ == "__main__":
    main()
