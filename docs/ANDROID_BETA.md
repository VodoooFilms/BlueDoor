# Beta Android

The signed public beta is pending; no release APK has been published by this
change. Download only a signed `BlueDoor-android-arm64-release.apk` from the
[project releases](https://github.com/VodoooFilms/BlueDoor/releases), once listed.
Do not install a preview or an unsigned build as a release.

1. Compare the downloaded APK SHA-256 with its `.sha256` sidecar; verify the
   release certificate fingerprint against the maintainer's published value.
2. On Android, open the APK and grant “Install unknown apps” to the browser or
   file manager you used. Install, then turn that permission off again.
3. Download the desired models explicitly in Models. Android Qwen analysis also
   requires GLiNER multilingual; no model is downloaded automatically.
4. Test with fictional documents first. Review every entity and page before
   sharing an export. Detection is fallible; pseudonymization is not anonymity.

If an existing preview uses another certificate, Android refuses an update.
Back up your own files manually before uninstalling it; uninstalling clears app
storage. Never migrate personal documents automatically.

## Report bugs voluntarily

Use [Android beta issues](https://github.com/VodoooFilms/BlueDoor/issues/new?template=android-beta.yml).
Include app/model version, phone model, Android version and reproduction steps
using fictional text. Screenshots and logs are optional. There is no telemetry,
remote crash reporting or automatic upload. GitHub receives only what you submit.

For optional logs, enable USB debugging temporarily and install Android platform
tools. Reproduce with a fictional document, then export only this app's process:

```sh
adb shell pidof -s com.bluedoor.desktop
# Replace PID below with that number, while the process is still running:
adb logcat --pid=PID -d -v threadtime > bluedoor-logcat.txt
```

This does not clear the device log buffer or upload anything. After a crash the
PID may be absent; reproduce first, rather than exporting every app's logs.
Read the local file and remove personal data, file paths, URIs and document
content before pasting a short relevant excerpt. Never attach an unreviewed raw
log. Disable USB debugging when done. You may report without logs.

For performance reports use only synthetic documents, include the device, model
ID/hash and cold/warm load distinction. `scripts/measure-android.py` records
resource metrics locally; it does not measure model loading by itself.
