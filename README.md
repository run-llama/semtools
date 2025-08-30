# SemTools

> Semantic search and document parsing tools for the command line

A collection of high-performance CLI tools for document processing and semantic search, built with Rust for speed and reliability.

- **`parse`** - Parse documents (PDF, DOCX, etc.) using, by default, the LlamaParse API into markdown format
- **`search`** - Local semantic keyword search using multilingual embeddings with cosine similarity matching and per-line context matching

**NOTE:** By default, `parse` uses LlamaParse as a backend. Get your API key today for free at [https://cloud.llamaindex.ai](https://cloud.llamaindex.ai). `search` remains local-only.

## Key Features

- **Fast semantic search** using model2vec embeddings, without the burden of a vector database
- **Reliable document parsing** with caching and error handling  
- **Unix-friendly** design with proper stdin/stdout handling
- **Configurable** distance thresholds and returned chunk sizes
- **Multi-format support** for parsing documents (PDF, DOCX, PPTX, etc.)
- **Concurrent processing** for better parsing performance

## Quick Start

Prerequisites:

- [Rust + Cargo](https://www.rust-lang.org/tools/install)
- For the `parse` tool: LlamaIndex Cloud API key

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
# Parse some files
parse my_dir/*.pdf

# Search some (text-based) files
search "some keywords" *.txt --max-distance 0.3 --n-lines 5

# Combine parsing and search
parse my_docs/*.pdf | xargs -n 1 search "API endpoints"
```

Advanced Usage:

```bash
# Combine with grep for exact-match pre-filtering and distance thresholding
parse *.pdf | xargs cat | grep -i "error" | search "network error" --max-distance 0.3

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs parse | xargs -n 1 search "installation"

# Combine with grep for filtering (grep could be before or after parse/search!)
parse docs/*.pdf | xargs -n 1 search "API" | grep -A5 "authentication"

# Save search results
parse report.pdf | xargs cat | search "summary" > results.txt
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
  <QUERY>     Query to search for (positional argument)
  [FILES]...  Files or directories to search

Options:
  -n, --n-lines <N_LINES>            How many lines before/after to return as context [default: 3]
      --top-k <TOP_K>                The top-k files or texts to return (ignored if max_distance is set) [default: 3]
  -m, --max-distance <MAX_DISTANCE>  Return all results with distance below this threshold (0.0+)
  -i, --ignore-case                  Perform case-insensitive search (default is false)
  -h, --help                         Print help
  -V, --version                      Print version
```

## Configuration

### Parse Tool Configuration

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

## Agent Use Case Examples

- [Using Semtools with Coding Agents](examples/use_with_coding_agents.md)
- [Using Semtools with MCP](examples/use_with_mcp.md)

## Future Work

- [ ] More parsing backends (something local-only would be great!)
- [ ] Allowing model selection for the search tool

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [LlamaIndex/LlamaParse](https://cloud.llamaindex.ai/) for document parsing capabilities
- [model2vec-rs](https://github.com/MinishLab/model2vec-rs)for fast embedding generation
- [simsimd](https://github.com/ashvardanian/simsimd) for efficient similarity computation 
