# Parse Tool

A high-performance CLI tool for parsing documents into markdown (using LlamaParse by default). Converts PDFs, DOCX, PPTX, and other formats into clean markdown with intelligent text extraction.

## Features

- **Multi-format support**: PDF, DOCX, PPTX, and more
- **Smart caching**: Avoids re-parsing unchanged or previously parsed files
- **Concurrent processing**: Parse multiple files simultaneously  
- **Configurable parsing**: Customize extraction parameters
- **Unix-friendly**: Outputs file paths for pipeline composition

## Installation

```bash
# Install the complete SemTools package
cargo install semtools
```

## CLI Usage

```bash
parse --help
A CLI tool for parsing documents using various backends

Usage: parse [OPTIONS] <FILES>...

Arguments:
  <FILES>...  Files to parse

Options:
  -c, --parse-config <PARSE_CONFIG>  Path to the config file. Defaults to ~/.parse_config.json
  -b, --backend <BACKEND>            The backend type to use for parsing. Defaults to `llama-parse` [default: llama-parse]
  -h, --help                         Print help
  -V, --version                      Print version
```

## Configuration

### API Key Setup

Get your API key from [LlamaIndex Cloud](https://cloud.llamaindex.ai):

**Option 1: Environment variable**
```bash
export LLAMA_CLOUD_API_KEY="your_api_key_here"
```

**Option 2: Config file**
Create `~/.parse_config.json`:
```json
{
  "api_key": "your_llama_cloud_api_key_here",
  "num_ongoing_requests": 10,
  "base_url": "https://api.cloud.llamaindex.ai",
  "check_interval": 5,
  "max_timeout": 3600,
  "parse_kwargs": {
    "parse_mode": "parse_page_with_agent",
    "model": "openai-gpt-4-1-mini",
    "high_res_ocr": "true",
    "adaptive_long_table": "true",
    "outlined_table_extraction": "true",
    "output_tables_as_HTML": "true"
  }
}
```

### Configuration Options

| Field | Default | Description |
|-------|---------|-------------|
| `api_key` | `$LLAMA_CLOUD_API_KEY` | Your LlamaIndex Cloud API key |
| `num_ongoing_requests` | `10` | Max concurrent parsing jobs |
| `base_url` | `https://api.cloud.llamaindex.ai` | API endpoint |
| `check_interval` | `5` | Polling interval in seconds |
| `max_timeout` | `3600` | Max wait time in seconds |
| `parse_kwargs` | See above | Parsing parameters |

## Usage

### Basic Usage

```bash
# Parse a single file
parse document.pdf

# Parse multiple files
parse file1.pdf file2.docx file3.pptx

# Parse with custom config
parse -c my_config.json document.pdf

# Verbose output
parse -v document.pdf
```

### Output

The tool outputs the paths to parsed markdown files, one per line:

```bash
$ parse report.pdf presentation.pptx
/home/user/.parse/report_abc123.md
/home/user/.parse/presentation_def456.md
```

### Pipeline Usage

**Search parsed content:**
```bash
# Search in parsed files (searches filenames)
parse document.pdf | search "revenue"

# Search file contents (note the 'cat')
parse document.pdf | cat | search "quarterly results"
```

**Process results:**
```bash
# Count lines in parsed files
parse *.pdf | xargs wc -l

# View first parsed file
parse document.pdf | head -1 | xargs cat

# Copy parsed files to a directory
parse *.pdf | xargs cp -t ./parsed_docs/
```

## Caching

Parsed files are cached in `~/.parse/` using content hashing:

- Files are only re-parsed if content changes
- Cache files are named with content hashes for deduplication
- Manual cache cleanup: `rm -rf ~/.parse/`

## Error Handling

The tool provides detailed error messages:

```bash
$ parse nonexistent.pdf
Warning: File does not exist: nonexistent.pdf

$ parse document.pdf  # without API key
Error: No API key provided. Set LLAMA_CLOUD_API_KEY or use config file.
```
