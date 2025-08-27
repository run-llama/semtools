# SemTools

> Semantic search and document parsing tools for the command line

A collection of high-performance CLI tools for document processing and semantic search, built with Rust for speed and reliability.

## Tools

- **`parse`** - Parse documents (PDF, DOCX, etc.) using, by default, the LlamaParse API into markdown format
- **`search`** - Semantic search using multilingual embeddings with cosine similarity matching and per-line context matching

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
# Parse a PDF and search for specific content
parse document.pdf | cat | search "error handling"

# Search within many files after parsing
parse my_docs/*.pdf | xargs -n 1 search "API endpoints"

# Search with custom context and thresholds
search "machine learning" *.txt --context 5 --threshold 0.3

# Search from stdin
echo "some text content" | search "content"
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
  "parse_kwargs": {
    "parse_mode": "parse_page_with_agent",
    "model": "openai-gpt-4-1-mini",
    "high_res_ocr": true,
    "adaptive_long_table": true,
    "outlined_table_extraction": true,
    "output_tables_as_HTML": true
  }
}
```

Or just set via environment variable:
```bash
export LLAMA_CLOUD_API_KEY="your_api_key_here"
```

## Usage Examples

### Basic Document Parsing and Search

```bash
# Parse multiple documents
parse report.pdf data.xlsx presentation.pptx

# Chain parsing with semantic search
parse *.pdf | cat | search "financial projections" --context 3

# Search with distance threshold (lower = more similar)
parse document.pdf | cat | search "revenue" --threshold 0.2
```

### Advanced Search Patterns

```bash
# Search multiple files directly
search "error handling" src/*.rs --top-k 5

# Combine with grep for exact-match pre-filtering and distance thresholding
parse *.pdf | xargs cat | grep -i "error" | search "network error" --threshold 0.3

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs parse | xargs cat | search "installation"
```

### Unix Pipeline Integration

The tools follow Unix philosophy and work seamlessly with standard tools:

```bash
# Combine with grep for filtering (could be before or after parse/search!)
parse docs/*.pdf | xargs -n 1 search "API" | grep -A5 "authentication"

# Use with xargs for batch processing
find . -name "*.pdf" | xargs parse | xargs -n 1 search "conclusion" 

# Save search results
parse report.pdf | search "summary" > results.txt
```

## Further Documentation

- [Parse Tool Documentation](crates/parse/README.md)
- [Search Tool Documentation](crates/search/README.md)

## Future Work

- [ ] More parsing backends (something local-only would be great!)
- [ ] Allowing model selection for the search tool

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [LlamaIndex/LlamaParse](https://cloud.llamaindex.ai/) for document parsing capabilities
- [model2vec](https://github.com/MinishLab/model2vec) for fast embedding generation
- [simsimd](https://github.com/ashvardanian/simsimd) for efficient similarity computation 
