#!/usr/bin/env python3
"""Collect device metrics only. Never reads documents, storage or logcat."""
import argparse, datetime, json, os, pathlib, re, subprocess, time
parser = argparse.ArgumentParser()
parser.add_argument('--seconds', type=int, default=180)
parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('artifacts/android-device-metrics.json'))
args = parser.parse_args()
if not 1 <= args.seconds <= 3600:
    parser.error('--seconds must be 1–3600')
adb = str(pathlib.Path(os.environ.get('ANDROID_HOME', str(pathlib.Path.home() / 'Library/Android/sdk'))) / 'platform-tools/adb')
def shell(*command):
    result = subprocess.run([adb, 'shell', *command], text=True, capture_output=True, timeout=15)
    return result.stdout.strip() if result.returncode == 0 else ''
subprocess.run([adb, 'get-state'], check=True, capture_output=True, timeout=10)
report = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'manufacturer': shell('getprop', 'ro.product.manufacturer'), 'model': shell('getprop', 'ro.product.model'),
          'android': shell('getprop', 'ro.build.version.release'), 'abi': shell('getprop', 'ro.product.cpu.abilist'),
          'pageSize': shell('getconf', 'PAGE_SIZE'), 'samples': []}
print('Mide solo operaciones con documentos sintéticos. Inicia OCR o análisis en el teléfono.', flush=True)
started = time.monotonic()
while time.monotonic() - started < args.seconds:
    memory = shell('dumpsys', 'meminfo', 'com.bluedoor.desktop')
    battery = shell('dumpsys', 'battery')
    def number(pattern, text):
        match = re.search(pattern, text)
        return int(match.group(1)) if match else None
    sample = {'seconds': round(time.monotonic() - started, 2),
              'totalPssKb': number(r'TOTAL PSS:\s*(\d+)', memory),
              'totalRssKb': number(r'TOTAL RSS:\s*(\d+)', memory),
              'batteryTemperatureTenthsC': number(r'temperature:\s*(\d+)', battery),
              'processPresent': bool(shell('pidof', 'com.bluedoor.desktop'))}
    report['samples'].append(sample)
    time.sleep(1)
report['thermalStatus'] = shell('dumpsys', 'thermalservice')
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
print(args.output.resolve())
