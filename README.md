# SemTools

> Semantic search and document parsing tools for the command line

A collection of high-performance CLI tools for document processing and semantic search, built with Rust for speed and reliability.

- **`parse`** - Parse documents (PDF, DOCX, etc.) using various backends (LlamaParse API or local LMStudio) into markdown format
- **`search`** - Local semantic keyword search using multilingual embeddings with cosine similarity matching and per-line context matching

**NOTE:** `parse` supports two backends:
- **LlamaParse** (default): Cloud API backend. Get your API key today for free at [https://cloud.llamaindex.ai](https://cloud.llamaindex.ai)
- **LMStudio**: Local backend using LMStudio for private, offline document parsing

`search` remains local-only.

## Key Features

- **Fast semantic search** using model2vec embeddings, without the burden of a vector database
- **Multiple parsing backends**: Choose between cloud (LlamaParse) and local (LMStudio) processing
- **Private document parsing**: LMStudio backend keeps your documents completely local
- **Reliable document parsing** with caching and error handling  
- **Unix-friendly** design with proper stdin/stdout handling
- **Configurable** distance thresholds and returned chunk sizes
- **Multi-format support** for parsing documents (PDF, DOCX, PPTX, etc.)
- **Concurrent processing** for better parsing performance

## Quick Start

Prerequisites:

- [Rust + Cargo](https://www.rust-lang.org/tools/install)
- For the `parse` tool with LlamaParse backend: LlamaIndex Cloud API key
- For the `parse` tool with LMStudio backend: [LMStudio](https://lmstudio.ai) with a loaded model

Install:

```bash
# install entire crate
cargo install semtools

# install only parse
cargo install semtools --no-default-features --features=parse

# install only search
cargo install semtools --no-default-features --features=search
```

Basic Usage:

```bash
# Parse some files (default: LlamaParse backend)
parse my_dir/*.pdf

# Parse with LMStudio backend (local)
parse my_dir/*.pdf --backend lmstudio

# Search some (text-based) files
search "some keywords" *.txt --max-distance 0.3 --n-lines 5

# Combine parsing and search
parse my_docs/*.pdf --backend lmstudio | xargs -n 1 search "API endpoints"
```

Advanced Usage:

```bash
# Combine with grep for exact-match pre-filtering and distance thresholding
parse *.pdf --backend lmstudio | xargs cat | grep -i "error" | search "network error" --max-distance 0.3

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs parse --backend lmstudio | xargs -n 1 search "installation"

# Combine with grep for filtering (grep could be before or after parse/search!)
parse docs/*.pdf --backend lmstudio | xargs -n 1 search "API" | grep -A5 "authentication"

# Save search results
parse report.pdf --backend lmstudio | xargs cat | search "summary" > results.txt

# Use different backends for different use cases
parse sensitive_docs/*.pdf --backend lmstudio  # Keep data local
parse public_docs/*.pdf --backend llama-parse  # Use cloud processing
```

## CLI Help

```bash
parse --help
A CLI tool for parsing documents using various backends

Usage: parse [OPTIONS] <FILES>...

Arguments:
  <FILES>...  Files to parse

Options:
  -c, --parse-config <PARSE_CONFIG>  Path to the config file. Defaults to ~/.parse_config.json
  -b, --backend <BACKEND>            The backend type to use for parsing. Defaults to `llama-parse` [default: llama-parse]
  -v, --verbose                      Verbose output while parsing
  -h, --help                         Print help
  -V, --version                      Print version
```

```bash
search --help
A CLI tool for fast semantic keyword search

Usage: search [OPTIONS] <QUERY> [FILES]...

Arguments:
  <QUERY>     
  [FILES]...  Files or directories to search

Options:
  -n, --n-lines <N_LINES>            [default: 3]
      --top-k <TOP_K>                [default: 3]
  -m, --max-distance <MAX_DISTANCE>  Return all results with distance below this threshold (0.0+)
  -h, --help                         Print help
  -V, --version                      Print version
```

## Configuration

### Parse Tool Configuration

The `parse` tool supports two backends with different configuration requirements:

#### LlamaParse Backend (Default)

By default, the `parse` tool uses the LlamaParse API to parse documents.

It will look for a `~/.parse_config.json` file to configure the API key and other parameters.

Otherwise, it will fallback to looking for a `LLAMA_CLOUD_API_KEY` environment variable and a set of default parameters.

To configure the `parse` tool, create a `~/.parse_config.json` file with the following content (defaults are shown below):

```json
{
  "api_key": "your_llama_cloud_api_key_here",
  "num_ongoing_requests": 10,
  "base_url": "https://api.cloud.llamaindex.ai",
  "check_interval": 5,
  "max_timeout": 3600,
  "max_retries": 10,
  "retry_delay_ms": 1000,
  "backoff_multiplier": 2.0,
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

Or just set via environment variable:
```bash
export LLAMA_CLOUD_API_KEY="your_api_key_here"
```

#### LMStudio Backend

The LMStudio backend requires a running LMStudio server with a loaded model.

**Setup:**
1. Download and install [LMStudio](https://lmstudio.ai)
2. Load a model (e.g., Llama, Mistral, Phi, etc.)
3. Start the local server (default port: 1234)

The backend will look for a `~/.lmstudio_parse_config.json` file with the following options:

```json
{
  "base_url": "http://localhost:1234/v1",
  "model": "llama-3.2-3b-instruct",
  "temperature": 0.3,
  "max_tokens": 4096,
  "chunk_size": 3000,
  "chunk_overlap": 200,
  "max_retries": 3,
  "retry_delay_ms": 1000,
  "num_ongoing_requests": 5
}
```

**Configuration Options:**
- `base_url`: LMStudio API endpoint (default: http://localhost:1234/v1)
- `model`: Model name as shown in LMStudio (get with `curl http://localhost:1234/v1/models`)
- `temperature`: Creativity level 0.0-1.0 (lower = more consistent)
- `max_tokens`: Maximum tokens per response
- `chunk_size`: Max characters per chunk for large documents  
- `chunk_overlap`: Overlap between chunks for context continuity
- `max_retries`: Retry attempts for failed requests
- `retry_delay_ms`: Delay between retries
- `num_ongoing_requests`: Number of concurrent processing tasks

**Environment Variable Support:**

You can also configure LMStudio backend via environment variables:
```bash
export LMSTUDIO_BASE_URL="http://localhost:1234/v1"
export LMSTUDIO_MODEL="your-model-name"
```

**Smart Config Loading:**

The LMStudio backend uses intelligent config loading:
1. Tries `~/.lmstudio_parse_config.json` (or your specified path)
2. Falls back to `~/.lmstudio_config.json`, `~/.lmstudio.json`
3. Falls back to general `~/.parse_config.json`
4. Uses environment variables if set
5. Uses built-in defaults as final fallback

## Agent Use Case Examples

- [Using Semtools with Coding Agents](examples/use_with_coding_agents.md)
- [Using Semtools with MCP](examples/use_with_mcp.md)

## Future Work

- [x] ~~More parsing backends (something local-only would be great!)~~ ✅ Added LMStudio backend
- [ ] Additional local parsing backends (Ollama, raw transformers)
- [ ] Allowing model selection for the search tool
- [ ] Support for more document formats in local backends

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [LlamaIndex/LlamaParse](https://cloud.llamaindex.ai/) for document parsing capabilities
- [model2vec-rs](https://github.com/MinishLab/model2vec-rs)for fast embedding generation
- [simsimd](https://github.com/ashvardanian/simsimd) for efficient similarity computation 
