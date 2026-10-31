# Adversarial PII regression corpus

**Scope: authored synthetic development fixtures, not real anonymized chats and
not a held-out benchmark.** 246 short chat-like examples (228 positive entities,
18 negative examples), Spanish LATAM/Chile and English wrappers. Many rows are
template variants of four values per category. These are useful regressions but
not 246 independent observations of real-world chat. No private conversations
were collected, anonymized or licensed on behalf of users. CC0-1.0 applies to
these fixtures; see LICENSE. An independent consented/legally sourced real-chat
corpus is still required before claiming the requested trust threshold.

`python3 tests/pii_eval/generate.py` deterministically recreates corpus.json;
there is no RNG, network access, timestamp or dependency on local personal data.
Values are fictional/testing examples, including invalid identifiers and the
standard card test numbers. Gold values must occur verbatim in the source.

Run `pnpm eval:pii`. This executes the production Rust rules and merge boundary
with empty learned-model suggestions. It fails when global complete-entity
byte coverage falls below 99%, and is explicitly invoked in CI. It does not run
Qwen or GLiNER; results must never be labeled full pipeline inference scores.
Normal `cargo test` ignores this explicit gate; CI invokes it separately.

Recall counts a gold entity only if every original UTF-8 byte is covered by
predicted source spans. Partial redaction is a miss. Precision counts unique
predicted spans only when they exactly match a gold value AND its category;
extra words, partial spans and predictions on negative examples are false
positives. Numeric category inference uses checksum/shape and production Kind;
this is an evaluation taxonomy, not new app-facing categories. Addresses that
include Markdown/punctuation outside gold reduce strict precision even when
all private bytes are covered. Overlapping generic number detections inside
validated structured spans are suppressed by production rules.

`baseline-rules.json`: observed rules-only coverage before changes, 89/228
(39.0%). Its initial report schema records extra spans but does not contain a
comparable strict precision score. It is not the historical 69%/61% result and
is not a Qwen baseline. `final-rules.json` contains the current exact-span
precision and per-category coverage; do not compare unlike precision metrics.
Set PII_EVAL_REPORT to an output path to retain a new machine-readable result.

Known limits: few independent values; no independent holdout, no real chats,
limited country-specific IBAN lengths, birth dates require explicit context,
RUT validity is a signal rather than proof of identity. Existing generic number
rules can classify order numbers and fragments of invalid cards as phones.
Checksums do not suppress explicit labeled private identifiers merely because
OCR changed a digit. This gate is a regression floor, not proof of anonymity
or of 99% recall in real conversations. Human review remains mandatory.
