#!/usr/bin/env python3
"""Run native CPU QA with fresh manifest downloads, bounded execution and RSS logs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def measured(command, destination, timeout, env=None):
    import psutil
    destination.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    peak = 0
    timed_out = False
    with (destination / 'process.log').open('w', encoding='utf-8') as log:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            while process.poll() is None:
                try:
                    parent = psutil.Process(process.pid)
                    rss = sum(p.memory_info().rss for p in [parent, *parent.children(recursive=True)] if p.is_running())
                    peak = max(peak, rss)
                except psutil.Error:
                    pass
                if time.monotonic() - start > timeout:
                    timed_out = True
                    try:
                        children = psutil.Process(process.pid).children(recursive=True)
                    except psutil.Error:
                        children = []
                    for child in children:
                        try:
                            child.kill()
                        except psutil.Error:
                            pass
                    try:
                        process.kill()
                    except ProcessLookupError:
                        pass
                    break
                time.sleep(0.2)
        finally:
            process.wait()
    result = {'command': command, 'seconds': time.monotonic() - start,
              'peak_process_tree_rss_bytes': peak, 'sample_interval_seconds': 0.2,
              'exit_code': process.returncode, 'timed_out': timed_out,
              'errors': [] if process.returncode == 0 and not timed_out else ['See process.log']}
    (destination / 'process.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    if process.returncode or timed_out:
        raise RuntimeError(f'QA failed: {destination / "process.log"}')


def download(asset, destination):
    start = time.monotonic()
    partial = destination.with_suffix('.part')
    digest = hashlib.sha256()
    count = 0
    try:
        with urllib.request.urlopen(asset['url'], timeout=60) as response, partial.open('wb') as out:
            while block := response.read(1024 * 1024):
                count += len(block)
                if count > asset['size']:
                    raise ValueError('Model larger than pinned manifest')
                digest.update(block)
                out.write(block)
        if count != asset['size'] or digest.hexdigest() != asset['sha256']:
            raise ValueError('Model integrity verification failed')
        partial.replace(destination)
    finally:
        partial.unlink(missing_ok=True)
    return {'bytes': count, 'sha256': digest.hexdigest(), 'seconds': time.monotonic() - start,
            'transport': 'CI urllib HTTPS; application downloader covered separately by existing tests'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--release', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    release = args.release.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    tools = {}
    for command in [['rustc', '--version'], ['cargo', '--version'], ['node', '--version'], ['cmake', '--version']]:
        tools[command[0]] = subprocess.check_output(command, text=True).splitlines()[0]
    (output / 'toolchain.json').write_text(json.dumps(tools, indent=2), encoding='utf-8')
    manifest = json.loads((ROOT / 'src-tauri/model-manifest.json').read_text())
    import psutil
    (output / 'host.json').write_text(json.dumps({
        'platform': __import__('platform').platform(), 'cpu': __import__('platform').processor(),
        'logical_cpus': os.cpu_count(), 'total_ram_bytes': psutil.virtual_memory().total,
        'inference': 'CPU only', 'physical_pc_validated': False, 'ram_4gb_validated': False
    }, indent=2), encoding='utf-8')
    measured(['cargo', 'test', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc',
              '--manifest-path', 'src-tauri/Cargo.toml', '--lib', '--no-run'], output / 'compile-tests', 1800)
    tests = list((release / 'deps').glob('bluedoor_lib-*.exe'))
    if len(tests) != 1:
        raise ValueError(f'Expected one test executable: {tests}')
    measured([str(tests[0]), '--test-threads=1'], output / 'native-suite', 900)
    # Never put weights inside resource paths or artifact upload paths.
    models = release.parent.parent / 'qa-models'
    models.mkdir(exist_ok=True)
    for model_id, filename in [('qwen3-fast', 'ggml-qwen3-1.7b-instruct-q4.gguf'),
                               ('qwen3-q8', 'ggml-qwen3-1.7b-instruct-q8.gguf')]:
        destination = output / model_id
        destination.mkdir(exist_ok=True)
        path = models / filename
        try:
            metadata = download(manifest[model_id], path)
            (destination / 'download.json').write_text(json.dumps(metadata, indent=2), encoding='utf-8')
            env = {**os.environ, 'BLUEDOOR_WINDOWS_QA_MODEL': str(path), 'BLUEDOOR_WINDOWS_QA_OUTPUT': str(destination)}
            measured([str(tests[0]), '--exact', 'privacy::tests::windows_x64_detect_protect_export',
                      '--ignored', '--nocapture', '--test-threads=1'], destination, 1200, env)
            # Independent PDF parser examines all objects/streams, including metadata.
            from pypdf import PdfReader
            reader = PdfReader(destination / 'protected.pdf')
            extracted = '\n'.join(page.extract_text() for page in reader.pages)
            corpus = extracted + '\n' + str(reader.metadata)
            for number in range(1, reader.trailer['/Size']):
                from pypdf.generic import IndirectObject
                obj = reader.get_object(IndirectObject(number, 0, reader))
                corpus += '\n' + str(obj)
                if hasattr(obj, 'get_data'):
                    data = obj.get_data()
                    corpus += data.decode('latin1') + data.decode('utf-16-be', errors='ignore')
            for value in ['Ana Perez', 'ana@example.com', '0912345678']:
                if ''.join(value.split()) in ''.join(corpus.split()):
                    raise ValueError(f'Export retains synthetic PII: {value}')
            if 'PERSON_' not in extracted:
                raise ValueError('Protected PDF has no person alias')
            (destination / 'independent-pdf.json').write_text(json.dumps({'passed': True, 'all_objects_checked': True}))
        except Exception as error:
            (destination / 'error.json').write_text(json.dumps({'error': str(error)}, indent=2), encoding='utf-8')
            raise
        finally:
            path.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
