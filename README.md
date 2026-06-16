# zolit

A CLI tool that exports Zotero annotations into companion markdown files. It reads directly from Zotero's SQLite database, matches annotations to your markdown notes by filename, and inserts them inline at their best-matching positions using a lightweight DSL embedded in HTML comments.

Designed for academic workflows where researchers maintain markdown notes alongside Zotero PDF collections (e.g. OCR'd papers). zolit bridges Zotero's annotation system and a plain-text knowledge base.

## Features

- **Direct Zotero database access** -- reads `zotero.sqlite` in read-only mode, safe to run while Zotero is open
- **Multi-stage text matching** -- exact substring, fuzzy (Levenshtein + LCS), OCR-aware normalization (ligatures, hyphens, confusable punctuation), and windowed multi-paragraph matching
- **Page-scoped matching** -- uses `<!-- Page N -->` markers from OCR tools to narrow search scope
- **Deduplication** -- tracks imported annotation IDs both in-file and via a `.zolit-sync.json` manifest; re-running sync is safe
- **Child notes** -- extracts Zotero child notes, converts HTML to Markdown, inserts under a `## Zotero Notes` heading
- **Dry-run mode** -- preview changes without writing files
- **LLM fallback** (optional) -- uses an OpenAI-compatible API to semantically place annotations that the text-matching pipeline couldn't resolve

### Supported annotation types

| Type | Supported |
|------|-----------|
| Highlight | Yes |
| Sticky note | Yes |
| Underline | Yes |
| Freetext | Yes |
| Image | No (not representable as text) |
| Ink/freehand | No |

## Installation

```bash
# Install from source
cargo install --path .

# With LLM fallback support
cargo install --path . --features llm
```

## Usage

```bash
# List all annotated PDFs in Zotero
zolit list --db ~/Zotero/zotero.sqlite

# Check sync status (matched/unmatched/pending counts)
zolit status --db ~/Zotero/zotero.sqlite --md-dir ~/notes/papers

# Preview what sync would do
zolit sync --md-dir ~/notes/papers --dry-run

# Sync annotations into markdown files
zolit sync --md-dir ~/notes/papers

# Sync with a stricter match threshold and filename filter
zolit sync --md-dir ~/notes/papers --threshold 0.6 --filter "Smith2024"

# Write results to a separate directory instead of modifying in-place
zolit sync --md-dir ~/notes/papers --output-dir ~/notes/papers-annotated
```

### Environment variables

Instead of passing `--db` and `--md-dir` every time, set:

```bash
export ZOLIT_DB=~/Zotero/zotero.sqlite
export ZOLIT_MD_DIR=~/notes/papers
zolit sync
```

## Subcommands

| Command | Description |
|---------|-------------|
| `sync` | Sync Zotero annotations into companion markdown files |
| `list` | List annotated PDFs found in the Zotero database |
| `status` | Show sync status: matched, unmatched, and pending counts |

### Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--db <PATH>` | `~/Zotero/zotero.sqlite` | Path to Zotero SQLite database |
| `--md-dir <PATH>` | *(required)* | Directory containing companion markdown files |
| `--output-dir <PATH>` | -- | Output directory (copy instead of modify in-place) |
| `--threshold <FLOAT>` | `0.4` | Fuzzy match threshold (0.0--1.0) |
| `--filter <PATTERN>` | -- | Filter markdown files by substring |
| `--dry-run` | `false` | Show what would change without writing |
| `-v` / `-vv` / `-vvv` | warn | Verbosity (info / debug / trace) |

With the `llm` feature enabled:

| Flag | Default | Description |
|------|---------|-------------|
| `--llm-fallback` | `false` | Use LLM for unmatched annotations |
| `--llm-model <NAME>` | `gpt-4o-mini` | Model name |
| `--llm-base-url <URL>` | `https://api.openai.com/v1` | API base URL (also `LLM_BASE_URL` env var) |

## Annotation DSL

Annotations are embedded as triple-dash HTML comments so they don't collide with standard `<!-- -->` comments.

Compact form (total length <= 120 chars):

```
<!---[zot-301] n: ^"highlighted text" | p. 5 -- highlight: my comment --->
```

Block form:

```
<!---[zot-301]
n:
^"highlighted text truncated to 60 chars..."
---
p. 5 -- highlight: my comment
--->
```

Unmatched annotations are collected under a `## Unmatched Zotero Annotations` heading at the end of the file.

## Matching pipeline

The matching pipeline is ordered from cheapest/most-precise to most expensive/tolerant, short-circuiting at the first hit:

1. **Exact substring** -- normalized whitespace and case
2. **Fuzzy single-paragraph** -- dual scoring via normalized Levenshtein distance and longest common substring (LCS) ratio
3. **OCR-enhanced** -- hyphenation rejoin, Unicode ligature normalization (`fi`, `fl`, `ffi`), confusable punctuation (curly quotes, em/en dashes)
4. **Windowed multi-paragraph** -- fuzzy match across 2--3 consecutive paragraphs

When markdown contains `<!-- Page N -->` markers, annotations are first matched within their page (+/- 1 page), falling back to full-document search.

## Manifest

zolit tracks sync state in `.zolit-sync.json` in the markdown directory:

```json
{
  "lastImport": "2024-01-01T00:00:00Z",
  "lastDbMtime": 1704067200,
  "entries": {
    "Smith2024_Deep_Learning": {
      "importedIds": ["zot-301", "zot-302", "zot-note-400"]
    }
  }
}
```

## License

MIT
