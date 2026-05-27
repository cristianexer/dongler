# Private Corpus Testing

Dongler supports private CI tests against a document corpus without committing
those files to the public repository.

The publish workflow downloads a private `.tar.gz` archive, verifies its
SHA-256 checksum, extracts it into the runner, and runs a smoke test against the
files. The smoke test does not print document contents.

## Required Secrets

- `DONGLER_CORPUS_URL`: private HTTPS URL for the archive.
- `DONGLER_CORPUS_SHA256`: expected SHA-256 checksum.
- `DONGLER_CORPUS_AUTH_HEADER`: optional HTTP auth header, such as
  `Authorization: Bearer ...`.

## Current Smoke Test

The current private corpus test only processes `.txt` and `.text` files because
the public extractor currently supports text extraction. It runs:

- `dongler inspect`
- `dongler extract --format markdown`
- `dongler extract --format json`
- `dongler extract --format latex`

When PDF extraction lands, add PDF fixtures to the private archive and extend the
smoke test to validate expected PDF outputs without exposing the corpus.
