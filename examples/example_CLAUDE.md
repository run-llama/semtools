# Augmented CLI Tooling

If executing bash commands, you have two very helpful utilities installed
- `parse` -- converts any non grep-able format into markdown, outputs a filepath for a converted markdown file for every input file to stdin
- `search` -- performs a search using static embeddings on either stdin or a list of files (very similar to grep). Works best with keyword based search queries. Only works with text-based files so it may require the `parse` tool to help preprocess into markdown.

These command, combined with other CLI commands, you can ensure that you can search large amounts of files efficiently, while handling various formats of documents. Both `parse` and `search` can scale to hundreds of thousands of documents.

## Parse CLI Help

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

## Search CLI Help

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

## Common Usage Patterns

```bash
# Parse a PDF and search for specific content
parse document.pdf | xargs cat | search "error handling"

# Search within many files after parsing
parse my_docs/*.pdf | xargs -n 1 search "API endpoints"

# Search with custom context and thresholds or distance thresholds
search "machine learning" *.txt --n-lines 5 --max-distance 0.3

# Search from stdin
echo "some text content" | search "content"

# Parse multiple documents
parse report.pdf data.xlsx presentation.pptx

# Chain parsing with semantic search
parse *.pdf | xargs -n 1 search "financial projections" --n-lines 3

# Search with distance threshold (lower = more similar)
parse document.pdf | xargs cat | search "revenue" --max-distance 0.2

# Search multiple files directly
search "error handling" src/*.rs --top-k 5

# Combine with grep for exact-match pre-filtering and distance thresholding
parse *.pdf | xargs cat | grep -i "error" | search "network error" --max-distance 0.3

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs parse | xargs -n 1 search "installation"
```

## Tips for using these tools

- `parse` will always output paths of parsed files to stdin. These parsed files represent the markdown version of their original file (for example, parsing a PDF or DOCX file into markdown).
- ALWAYS call `parse` first when interacting with PDF (or similar) formats so that you can get the paths to the markdown versions of those files
- `search` only works with text-based files (like markdown). It's a common pattern to first call `parse` and either feed files into `search` or cat files and search from stdin
- `search` works best with keywords, or comma-separated inputs
- `--n-lines` on search controls how much context is shown around matching lines in the results
- `--max-distance` is useful on search for cases where you don't know a top-k value ahead of time and need relevant results from all files