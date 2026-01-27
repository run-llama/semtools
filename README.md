# SemTools

> Semantic search and document parsing tools for the command line

A collection of high-performance CLI tools for document processing and semantic search, built with Rust for speed and reliability.

- **`parse`** - Parse documents (PDF, DOCX, etc.) using, by default, the LlamaParse API into markdown format
- **`search`** - Local semantic keyword search using multilingual embeddings with cosine similarity matching and per-line context matching
- **`ask`** - AI agent with search and read tools for answering questions over document collections (defaults to OpenAI, but see the [config section](#configuration) to learn more about connecting to any OpenAI-Compatible API)
- **`workspace`** - Workspace management for accelerating search over large collections

**NOTE:** By default, `parse` uses LlamaParse as a backend. Get your API key today for free at [https://cloud.llamaindex.ai](https://cloud.llamaindex.ai). `search` and `workspace` remain local-only. `ask` requires an OpenAI API key.

## Key Features

- **Fast semantic search** using model2vec embeddings from [minishlab/potion-multilingual-128M](https://huggingface.co/minishlab/potion-multilingual-128M)
- **Reliable document parsing** with caching and error handling  
- **Unix-friendly** design with proper stdin/stdout handling
- **Configurable** distance thresholds and returned chunk sizes
- **Multi-format support** for parsing documents (PDF, DOCX, PPTX, etc.)
- **Concurrent processing** for better parsing performance
- **Workspace management** for efficient document retrieval over large collections

## Installation

Prerequisites:

- For the `parse` tool: LlamaIndex Cloud API key

Install:

You can install `semtools` via npm:

```bash
npm i -g @llamaindex/semtools
```

Or via cargo:

```bash
# install entire crate
cargo install semtools

# install only select features
cargo install semtools --no-default-features --features=parse
```

Note: Installing from npm builds the Rust binaries locally during install if a prebuilt binary is not available, which requires Rust and Cargo to be available in your environment. Install from `rustup` if needed: `https://www.rust-lang.org/tools/install`.

## Quick Start

Basic Usage:

```bash
# Parse some files
parse my_dir/*.pdf

# Search some (text-based) files
search "some keywords" *.txt --max-distance 0.3 --n-lines 5

# Ask questions about your documents using an AI agent
ask "What are the main findings?" papers/*.txt

# Combine parsing and search
parse my_docs/*.pdf | xargs search "API endpoints"

# Ask a question to a set of files
ask "Some question?" *.txt 

# Combine parsing with the ask agent
parse research_papers/*.pdf | xargs ask "Summarize the key methodologies"

# Ask based on stdin content
cat README.md | ask "How do I install SemTools?"
```

Advanced Usage:

```bash
# Combine with grep for exact-match pre-filtering and distance thresholding
parse *.pdf | xargs cat | grep -i "error" | search "network error" --max-distance 0.3

# Pipeline with content search (note the 'xargs' on search to search files instead of stdin)
find . -name "*.md" | xargs parse | xargs search "installation"

# Combine with grep for filtering (grep could be before or after parse/search!)
parse docs/*.pdf | xargs search "API" | grep -A5 "authentication"

# Save search results from stdin search
parse report.pdf | xargs cat | search "summary" > results.txt
```

Using Workspaces:

```bash
# Create or select a workspace
# Workspaces are stored in ~/.semtools/workspaces/
workspace use my-workspace
> Workspace 'my-workspace' configured.
> To activate it, run:
>   export SEMTOOLS_WORKSPACE=my-workspace
> 
> Or add this to your shell profile (.bashrc, .zshrc, etc.)

# Activate the workspace
export SEMTOOLS_WORKSPACE=my-workspace

# All search commands will now use the workspace for caching embeddings
# The initial command is used to initialize the workspace
search "some keywords" ./some_large_dir/*.txt --n-lines 5 --top-k 10

# If documents change, they are automatically re-embedded and cached
echo "some new content" > ./some_large_dir/some_file.txt
search "some keywords" ./some_large_dir/*.txt --n-lines 5 --top-k 10

# If documents are removed, you can run prune to clean up stale files
workspace prune

# You can see the stats of a workspace at any time
workspace status
> Active workspace: arxiv
> Root: /Users/loganmarkewich/.semtools/workspaces/arxiv
> Documents: 3000
> Index: Yes (IVF_PQ)
```

## CLI Help

```bash
$ parse --help
A CLI tool for parsing documents using various backends

Usage: parse [OPTIONS] <FILES>...

Arguments:
  <FILES>...  Files to parse

Options:
  -c, --config <CONFIG>    Path to the config file. Defaults to ~/.semtools_config.json
  -b, --backend <BACKEND>  The backend type to use for parsing. Defaults to `llama-parse` [default: llama-parse]
  -v, --verbose            Verbose output while parsing
  -h, --help               Print help
  -V, --version            Print version
```

```bash
$ search --help
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

```bash
$ workspace --help
Manage semtools workspaces

Usage: workspace <COMMAND>

Commands:
  use     Use or create a workspace (prints export command to run)
  status  Show active workspace and basic stats
  prune   Remove stale or missing files from store
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

```bash
$ ask --help
A CLI tool for fast semantic keyword search

Usage: ask [OPTIONS] <QUERY> [FILES]...

Arguments:
  <QUERY>     Query to prompt the agent with
  [FILES]...  Files to search, optional if using stdin

Options:
  -c, --config <CONFIG>      Path to the config file. Defaults to ~/.semtools_config.json
      --api-key <API_KEY>    OpenAI API key (overrides config file and env var)
      --base-url <BASE_URL>  OpenAI base URL (overrides config file)
  -m, --model <MODEL>        Model to use for the agent (overrides config file)
  -h, --help                 Print help
  -V, --version              Print version
```

## Configuration

SemTools uses a unified configuration file at `~/.semtools_config.json` that contains settings for all CLI tools. You can also specify a custom config file path using the `-c` or `--config` flag on any command.

### Unified Configuration File

Create a `~/.semtools_config.json` file with settings for the tools you use. All sections are optional - if not specified, sensible defaults will be used.

```json
{
  "parse": {
    "api_key": "your_llama_cloud_api_key_here",
    "num_ongoing_requests": 10,
    "base_url": "https://api.cloud.llamaindex.ai",
    "parse_kwargs": {
      "tier": "agentic",
      "version": "latest",
      "processing_options": {
          "ignore": {
              "ignore_diagonal_text": true,
              "ignore_text_in_image": false
          },
          "ocr_parameters": {
              "languages": ["en", "es"]
          }
      },
      "agentic_options": {
          "custom_prompt": "Translate everything to French"
      },
      "page_ranges": {
          "max_pages": 20,
          "target_pages": "1-5,10,15-20"
      },
      "crop_box": {
          "top": 0.05,
          "bottom": 0.95,
          "left": 0.05,
          "right": 0.95
      },
      "output_options": {
          "markdown": {
              "annotate_links": true,
              "tables": {
                "output_tables_as_markdown": true
              }
          },
          "images_to_save": ["screenshot"]
      },
      "webhook_configurations": [
          {
            "webhook_url": "https://example.com/webhook",
            "webhook_events": ["parse.done"]
          }
        ],
      "processing_control": {
          "timeouts": {
              "base_in_seconds": 600
          },
          "job_failure_conditions": {
              "allowed_page_failure_ratio": 0.05
            }
      },
      "disable_cache": false
    },
    "check_interval": 5,
    "max_timeout": 3600,
    "max_retries": 10,
    "retry_delay_ms": 1000,
    "backoff_multiplier": 2.0
  },
  "ask": {
    "api_key": "your_openai_api_key_here",
    "base_url": null,
    "model": "gpt-4o-mini",
    "max_iterations": 20,
    "api_mode": "responses",  // Can be responses or chat
  }
}
```

Find out more about parsing configuration [on the dedicated documentation page](https://developers.llamaindex.ai/python/cloud/llamaparse/api-v2-guide/).

See `example_semtools_config.json` in the repository for a complete example.

### Environment Variables

As an alternative or supplement to the config file, you can set API keys via environment variables:

```bash
# For parse tool
export LLAMA_CLOUD_API_KEY="your_llama_cloud_api_key_here"

# For ask tool
export OPENAI_API_KEY="your_openai_api_key_here"
```

### Configuration Priority

Configuration values are resolved in the following priority order (highest to lowest):

1. **CLI arguments** (e.g., `--api-key`, `--model`, `--base-url`)
2. **Config file** (`~/.semtools_config.json` or custom path via `-c`)
3. **Environment variables** (`LLAMA_CLOUD_API_KEY`, `OPENAI_API_KEY`)
4. **Built-in defaults**

This allows you to set common defaults in the config file while overriding them on a per-command basis when needed.

### Tool-Specific Configuration

#### Parse Tool

The `parse` tool requires a LlamaParse API key. Get your free API key at [https://cloud.llamaindex.ai](https://cloud.llamaindex.ai).

Configuration options:
- `api_key`: Your LlamaParse API key
- `base_url`: API endpoint (default: "https://api.cloud.llamaindex.ai")
- `num_ongoing_requests`: Number of concurrent requests (default: 10)
- `parse_kwargs`: Additional parsing parameters
- `check_interval`, `max_timeout`, `max_retries`, `retry_delay_ms`, `backoff_multiplier`: Retry and timeout settings

#### Ask Tool

The `ask` tool requires an OpenAI API key for the agent's LLM.

Configuration options:
- `api_key`: Your OpenAI API key
- `base_url`: Custom OpenAI-compatible API endpoint (optional, for using other providers)
- `model`: LLM model to use (default: "gpt-4o-mini")
- `max_iterations`: Maximum agent loop iterations (default: 10)

You can also override these per-command:
```bash
ask "What is this about?" docs/*.txt --model gpt-4o --api-key sk-...
```

## Agent Use Case Examples

- [Using Semtools with Coding Agents](examples/use_with_coding_agents.md)
- [Using Semtools with MCP](examples/use_with_mcp.md)

## Future Work

- [ ] More parsing backends (something local-only would be great!)
- [ ] Improved search algorithms
- [x] Built-in agentic search
- [x] Persistence for speedups on repeat searches on the same files 

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [LlamaIndex/LlamaParse](https://cloud.llamaindex.ai/) for document parsing capabilities
- [model2vec-rs](https://github.com/MinishLab/model2vec-rs)for fast embedding generation
- [minishlab/potion-multilingual-128M](https://huggingface.co/minishlab/potion-multilingual-128M) for an amazing default static embedding model 
- [simsimd](https://github.com/ashvardanian/simsimd) for efficient similarity computation
