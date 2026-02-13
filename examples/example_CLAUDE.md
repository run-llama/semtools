# Augmented CLI Tooling

If executing bash commands, you have three very helpful sucommands installed within `semtools`:
- `parse` -- converts any non grep-able format into markdown, outputs a filepath for a converted markdown file for every input file to stdin
- `search` -- performs a search using static embeddings on either stdin or a list of files (very similar to grep). Works best with keyword based search queries. Only works with text-based files so it may require the `parse` tool to help preprocess into markdown.
- `workspace` - Workspace management for accelerating search over large collections

These commands, combined with other CLI commands, you can ensure that you can search large amounts of files efficiently, while handling various formats of documents. Both `parse` and `search` can scale to hundreds of thousands of documents.

## Parse Subcommand Help

```bash
$ semtools parse --help
A CLI tool for parsing documents using various backends

Usage: semtools parse [OPTIONS] <FILES>...

Arguments:
  <FILES>...  Files to parse

Options:
  -c, --config <CONFIG>    Path to the config file. Defaults to ~/.semtools_config.json
  -b, --backend <BACKEND>  The backend type to use for parsing. Defaults to `llama-parse` [default: llama-parse]
  -v, --verbose            Verbose output while parsing
  -h, --help               Print help
```

## Search Subcommand Help

```bash
$ semtools search --help
A CLI tool for fast semantic keyword search

Usage: semtools search [OPTIONS] <QUERY> [FILES]...

Arguments:
  <QUERY>     Query to search for (positional argument)
  [FILES]...  Files to search, optional if using stdin

Options:
  -n, --n-lines <N_LINES>            How many lines before/after to return as context [default: 3]
      --top-k <TOP_K>                The top-k files or texts to return (ignored if max_distance is set) [default: 3]
  -m, --max-distance <MAX_DISTANCE>  Return all results with distance below this threshold (0.0+)
  -i, --ignore-case                  Perform case-insensitive search (default is false)
  -j, --json                         Output results in JSON format
  -h, --help                         Print help
```

## Workspaces Subcommand Help

```bash
$ semtools workspace --help
Manage semtools workspaces

Usage: semtools workspace [OPTIONS] <COMMAND>

Commands:
  use     Use or create a workspace (prints export command to run)
  status  Show active workspace and basic stats
  prune   Remove stale or missing files from store
  help    Print this message or the help of the given subcommand(s)

Options:
  -j, --json  Output results in JSON format
  -h, --help  Print help
```


## Common Usage Patterns

Here's how to convert those standalone commands to `semtools` subcommands:

### Using Parse and Search

```bash
# Parse a PDF and search for specific content
semtools parse document.pdf | xargs cat | semtools search "error handling"

# Search within many files after parsing
semtools parse my_docs/*.pdf | xargs semtools search "API endpoints"

# Search with custom context and thresholds or distance thresholds
semtools search "machine learning" *.txt --n-lines 5 --max-distance 0.3

# Search from stdin
echo "some text content" | semtools search "content"

# Parse multiple documents
semtools parse report.pdf data.xlsx presentation.pptx

# Chain parsing with semantic search
semtools parse *.pdf | xargs semtools search "financial projections" --n-lines 3

# Search with distance threshold (lower = more similar)
semtools parse document.pdf | xargs cat | semtools search "revenue" --max-distance 0.2

# Search multiple files directly
semtools search "error handling" src/*.rs --top-k 5

# Combine with grep for exact-match pre-filtering and distance thresholding
semtools parse *.pdf | xargs cat | grep -i "error" | semtools search "network error" --max-distance 0.3

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs semtools parse | xargs semtools search "installation"
```

### Using with Workspaces

```bash
# Create or select a workspace
# Workspaces are stored in ~/.semtools/workspaces/
semtools workspace use my-workspace
> Workspace 'my-workspace' configured.
> To activate it, run:
>   export SEMTOOLS_WORKSPACE=my-workspace
> 
> Or add this to your shell profile (.bashrc, .zshrc, etc.)

# Activate the workspace
export SEMTOOLS_WORKSPACE=my-workspace

# All search commands will now use the workspace for caching embeddings
# The initial command is used to initialize the workspace
semtools search "some keywords" ./some_large_dir/*.txt --n-lines 5 --top-k 10

# If documents change, they are automatically re-embedded and cached
echo "some new content" > ./some_large_dir/some_file.txt
semtools search "some keywords" ./some_large_dir/*.txt --n-lines 5 --top-k 10

# A workspace example if you are using with parse
# create a workspace
semtools workspace use my-workspace2
export SEMTOOLS_WORKSPACE=my-workspace2

# parse files, and then search over the parsed files, and cache the file embeddings
semtools parse *.pdf | xargs semtools search "financial projections" --n-lines 3

# if you run the command with a different query (see option a and b), over the same set of files, then search will operate
# over the cached file embeddings
# option a - parse won't rerun since files already cached
semtools parse *.pdf | xargs semtools search "balance sheet" --n-lines 3

# option b - run search directly over the parse cache
xargs semtools search "balance sheet" /Users/jerryliu/.parse/*.pdf.md --n-lines 3  

# If documents are removed, you can run prune to clean up stale files
semtools workspace prune

# You can see the stats of a workspace at any time
semtools workspace status
> Active workspace: arxiv
> Root: /Users/loganmarkewich/.semtools/workspaces/arxiv
> Documents: 3000
> Index: Yes (IVF_PQ)
```

## Tips for using these tools

- If you have run / plan on running repeated `search` queries over the same file or set of files, you SHOULD create a workspace (`semtools workspace use`) before running parse/search commands - otherwise you will be re-embedding the same document collections from scratch every time. Make sure the environment variable is set before downstream commands.
- Before you create a workspace, you can check current workspace through `semtools workspace status` which will also give the directory where all workspaces are stored.
- You can choose to add a new workspace or prune an existing one if you are changing to a different collection of files. You can check the status through `semtools workspace status`.
- `parse` will always output paths of parsed files to stdin. These parsed files represent the markdown version of their original file (for example, parsing a PDF or DOCX file into markdown).
- ALWAYS call `parse` first when interacting with PDF (or similar) formats so that you can get the paths to the markdown versions of those files
- `search` only works with text-based files (like markdown). It's a common pattern to first call `parse` and either feed files into `search` or cat files and search from stdin
- `search` works best with keywords, or comma-separated inputs
- By default the tokenizer for `search` is case sensitive, which may lead to unexpected results if you don't know capitalization beforehand. You should generally TRY to set `--ignore-case` for more general case insensitive search.
- `--n-lines` on search controls how much context is shown around matching lines in the results
- If `--n-lines` returns incomplete results, you may want to consider expanding `--n-lines`.
- NOTE: by default --n-lines is too small. Consider setting n-lines to 30-50 at least always. 
- `--max-distance` is useful on search for cases where you don't know a top-k value ahead of time and need relevant results from all files
- That said if setting `--max-distance` doesn't return any results, you may want to try `--top-k` to double-check.
