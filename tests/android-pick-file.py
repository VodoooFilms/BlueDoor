#!/usr/bin/env python3
"""Choose a named synthetic fixture in an already-open Android system picker."""
import os, pathlib, re, subprocess, sys, time, xml.etree.ElementTree as ET
name = sys.argv[1]
if name not in ('bluedoor-synthetic-scan.png', 'bluedoor-synthetic-scan.pdf'):
    raise SystemExit('Only named synthetic OCR fixtures are permitted')
adb = str(pathlib.Path(os.environ.get('ANDROID_HOME', str(pathlib.Path.home() / 'Library/Android/sdk'))) / 'platform-tools/adb')
for attempt in range(8):
    subprocess.run([adb, 'shell', 'uiautomator', 'dump', '/sdcard/bluedoor-qa-picker.xml'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
    xml = subprocess.check_output([adb, 'exec-out', 'cat', '/sdcard/bluedoor-qa-picker.xml'], timeout=10)
    try:
        for node in ET.fromstring(xml).iter('node'):
            if node.get('text') == name and node.get('enabled') == 'true':
                bounds = [int(v) for v in re.findall(r'\d+', node.get('bounds', ''))]
                if len(bounds) == 4:
                    subprocess.run([adb, 'shell', 'input', 'tap', str((bounds[0] + bounds[2]) // 2), str((bounds[1] + bounds[3]) // 2)], check=True)
                    sys.exit(0)
    except ET.ParseError:
        pass
    time.sleep(.5)
raise SystemExit('Fixture not visible. Open Downloads/Recent in the system picker and retry.')
