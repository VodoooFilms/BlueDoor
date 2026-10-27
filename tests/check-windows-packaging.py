#!/usr/bin/env python3
"""Portable regression tests for the x64 installer runtime allowlist."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('package_windows', Path(__file__).resolve().parents[1] / 'scripts/package-windows-x64.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def pe(path, machine=0x8664):
    path.parent.mkdir(parents=True, exist_ok=True)
    data = bytearray(128)
    data[:2] = b'MZ'
    data[0x3c:0x40] = (64).to_bytes(4, 'little')
    data[64:68] = b'PE\0\0'
    data[68:70] = machine.to_bytes(2, 'little')
    path.write_bytes(data)


class RuntimePackaging(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        package.ROOT = self.root
        self.target = self.root / 'target'
        self.release = self.target / 'x86_64-pc-windows-msvc/release'
        self.native = self.release / 'build/llama-cpp-sys-2-current/out'
        for name in ['llama.dll', 'ggml.dll', 'ggml-base.dll']:
            pe(self.native / 'bin' / name)
        pe(self.native / 'backends/ggml-cpu-x64.dll')
        self.redist = self.root / 'redist'
        for name in ['vcruntime140.dll', 'msvcp140.dll']:
            pe(self.redist / 'x64/Microsoft.VC143.CRT' / name)

    def test_complete_runtime_is_mapped_next_to_exe_and_tests(self):
        package.stage(self.target, self.redist)
        config = json.loads((self.target / 'bundle-x64.json').read_text())
        resources = config['bundle']['resources']
        self.assertEqual(resources[str((self.release / 'llama.dll').resolve())], 'llama.dll')
        self.assertTrue((self.release / 'deps/ggml-cpu-x64.dll').is_file())
        self.assertEqual(config['bundle']['targets'], ['nsis'])
        # Repackaging an unchanged build must remain safe and deterministic.
        package.stage(self.target, self.redist)

    def test_arm_dll_is_rejected(self):
        pe(self.native / 'bin/llama.dll', 0xaa64)
        with self.assertRaisesRegex(ValueError, 'Not x64'):
            package.stage(self.target, self.redist)

    def test_gpu_backend_is_rejected(self):
        pe(self.native / 'backends/ggml-vulkan.dll')
        with self.assertRaisesRegex(ValueError, 'Non-CPU'):
            package.stage(self.target, self.redist)

    def test_missing_baseline_backend_is_rejected(self):
        (self.native / 'backends/ggml-cpu-x64.dll').unlink()
        with self.assertRaisesRegex(ValueError, 'Missing required'):
            package.stage(self.target, self.redist)

    def test_stale_ambiguous_builds_are_rejected(self):
        pe(self.release / 'build/llama-cpp-sys-2-stale/out/bin/llama.dll')
        with self.assertRaisesRegex(ValueError, 'one fresh llama'):
            package.stage(self.target, self.redist)

    def test_model_weights_are_never_packaged(self):
        weights = self.root / 'src-tauri/resources/runtime/accidental.gguf'
        weights.parent.mkdir(parents=True)
        weights.write_bytes(b'GGUF')
        with self.assertRaisesRegex(ValueError, 'weights must never'):
            package.stage(self.target, self.redist)

    def test_macos_runtime_is_never_packaged(self):
        binary = self.root / 'src-tauri/resources/runtime/bluedoor-ocr'
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b'Mach-O')
        with self.assertRaisesRegex(ValueError, 'Foreign macOS'):
            package.stage(self.target, self.redist)


if __name__ == '__main__':
    unittest.main()
