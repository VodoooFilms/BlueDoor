# Privacy Policy — BlueDoor

**Short version:** BlueDoor processes documents, audio and edits locally on your Mac.
After model installation, its document and transcription workflows can run offline.
The application downloads models when you request them. The optional **Send to GPT**
button opens ChatGPT in your browser; it does not upload a document, copy its text
or attach an export. If you manually upload an exported file there, that separate
online service processes it under its own terms and privacy settings.

## Private AI documents

Documents imported into Private AI are read and analyzed locally by Qwen3. The original, entity map and sanitized text remain in memory for the session; they are not added to the transcript history or written to a document database. Closing the app discards this session. The user's source file remains on disk. The preview is local and sends nothing; it contains only sanitized text and a schema version, with no filename or entity map.

Pseudonymization is not an anonymity guarantee. Human review and deterministic pattern checks are mandatory for the preview, but both the model and the checks can miss information or indirect clues. PDF/DOCX extraction and on-device OCR may be incomplete or misread sensitive details. Memory can remain in operating-system swap or dumps; this demo does not implement memory encryption or secure erasure. See `PRIVATE_AI.md` for limits.

### What the sanitizer refuses to do

The app requires a reviewed entity map before it approves, previews or exports a protected document. These checks cannot prove that every sensitive detail has been found. A review is rejected unless the entity map is non-empty **and** the sanitized text differs from the original, so a document in which nothing was detected can never be presented as safe by omission. When the map contains no person, the interface says so explicitly, because names are the one category no deterministic rule can check.

Exports to PDF are validated in the application process, not in the interface: the writer only accepts a document whose text is identical to the one that was reviewed, and it refuses the file if the session is not reviewed, the map is empty, or the requested text is not the reviewed one. Selecting a file and pressing Export is not sufficient on its own. The title is checked by the same rules, because it is printed in the page header and stored in the file's metadata.

## What we process

- **Audio** (live recordings and imported files) is converted to text locally by the
  Whisper model that runs on your machine. Microphone PCM is decoded in memory; temporary decoded imports are removed after
  transcription.
- **Transcriptions & corrections** are generated and edited locally by Whisper and edited by you. Transcripts you keep are stored locally — as plain JSON in
  `transcripts.json` inside the app's Application Support folder — and **never leave
  your device**.
- **PDF files** are created by the app only at the destination you pick in the native
  save dialog.

## What leaves your device

- **Model downloads**: when you press "Download" in the Models window,
  the app fetches the selected Whisper, Parakeet or Nemotron model file from
  `huggingface.co` and its download/CDN infrastructure. Requests disclose connection metadata such as IP address and the model requested; they do not include your audio, transcripts or documents. Qwen3 is fetched from `registry.ollama.ai` and its CDN as immutable public weights, also with no document content in the request. Model files are stored in
  `~/Library/Application Support/com.bluedoor.desktop/models/`.
- **Browser navigation**: pressing **Send to GPT** opens the fixed address
  `https://chatgpt.com/`. The browser makes its own network requests and may use
  your existing ChatGPT session. BlueDoor supplies no document, title, file path
  or text to this command. Attaching an exported PDF remains a manual action.
- No telemetry, analytics, application accounts or crash reporters are configured
  in BlueDoor. Its local processing workflows do not send document content to a server.

## Permissions the app may ask for

- **Microphone**: required to record your voice. Audio stays on your Mac and is
  deleted after transcription.
- **Files**: only what you explicitly open (import audio or private documents) or save (export PDF)
  through the native dialogs. The app does not scan your folders.

## Where private data lives in memory

The private document is held in the application process. The interface also
displays the original text next to the sanitized text, because comparing the
extraction against the source is what makes the review meaningful. The unmodified
OCR extraction is not sent with it: it is fetched only if you ask to see it.

## Temporary files

Imported media is decoded by the bundled, file-only decoder into a private temporary directory
that is removed when the operation finishes. Microphone PCM is decoded directly in memory. The bundled decoder has no network protocols. On startup the app also clears any leftover
partial downloads (`.part`) and temporary audio from interrupted sessions.

## Deletion

- Transcripts are stored locally until you delete them: use the **×** (trash) on any
  history item to remove it, or delete the whole `transcripts.json` file mentioned
  above to wipe all history.
- Downloaded models can be deleted from inside the app (trash icon in the Models
  window), or by removing the `models` folder shown above.
- Closing the private document (button "Cerrar documento") or quitting the app
  discards the private document and its entity map. Only one copy of the app runs
  at a time, so there is no second instance holding the same document.

## Distribution & security notes

- Current test builds are **ad-hoc signed** (no Apple Developer ID) and run
  **unsandboxed**. App Sandbox is separate from Developer ID signing; model downloads do not inherently require disabling it. If you
  obtain the app from someone else, only run builds you trust.
- Current artifacts are not yet notarized. Gatekeeper may block browser-downloaded test builds. Developer ID/notarization and clean-Mac acceptance remain release gates; users are not asked to run Terminal commands, disable system security or install dependencies.

## Contact

Questions about this policy: open an issue in this repository or contact the
repository owner.
