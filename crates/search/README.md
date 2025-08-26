# Search Tool

A semantic search CLI tool that uses multilingual embeddings to find relevant content. Unlike traditional grep, it understands meaning and context, making it perfect for finding conceptually similar text.

## Features

- **Semantic understanding**: Finds conceptually similar content, not just exact matches
- **Multilingual support**: Works across different languages  
- **Unix-friendly**: Reads from files or stdin, works in pipelines
- **Configurable relevance**: Adjust similarity thresholds and result counts
- **Context-aware**: Returns surrounding lines for better understanding
- **Fast embeddings**: Uses model2vec for efficient similarity computation

## Installation

```bash
# Install from the workspace root
cargo install --path crates/search

# Or install the entire semtools suite  
cargo install --path .
```

## Usage

```bash
search --help
Usage: search [OPTIONS] <QUERY> [FILES]...

Arguments:
  <QUERY>     
  [FILES]...  Files or directories to search

Options:
  -c, --context <CONTEXT>      [default: 3]
      --top-k <TOP_K>          [default: 3]
  -t, --threshold <THRESHOLD>  Return all results with distance below this threshold (0.0-1.0)
  -h, --help                   Print help
  -V, --version                Print version
loganmarkewich@Mac semtools % 
```

### Basic Syntax

```bash
search <query> [files...] [options]
```

### Command Line Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--context` | `-c` | `3` | Lines of context before/after matches |
| `--top-k` | | `3` | Maximum number of results to return |
| `--threshold` | `-t` | none | Distance threshold (0.0+, lower = more similar) |

### Basic Examples

```bash
# Search in files
search "error handling" src/main.rs src/lib.rs

# Search with more context
search "database connection" --context 5 *.rs

# Get top 10 results
search "authentication" --top-k 10 docs/*.md

# Use distance threshold (return all results under 0.3)
search "machine learning" --threshold 0.3 papers/*.txt
```

### Stdin Usage

```bash
# Search from pipe
echo "This is a test document about neural networks" | search "AI"

# Search file contents (note: use 'cat' to read file contents)
parse document.pdf | cat | search "revenue projections"

# Search in multiple parsed files
find . -name "*.md" | xargs cat | search "conclusion"
```

## Understanding Search Results

### Output Format

```
filename:start_line::end_line (distance)
[context lines with match]

```

### Example Output

```bash
$ search "error handling" src/main.rs
src/main.rs:45::51 (0.23)
    // Set up logging
    env_logger::init();
    
    // Main error handling logic
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

src/main.rs:67::73 (0.31)
    match config.validate() {
        Ok(_) => println!("Config valid"),
        // Handle configuration errors
        Err(e) => {
            eprintln!("Config error: {}", e);
            return Err(e);
        }
    }
```

## Semantic vs Exact Search

### Semantic Search (this tool)
```bash
# These queries would find similar results:
search "error handling"
search "exception management" 
search "fault tolerance"
search "failure recovery"
```

### Exact Search (grep)
```bash
# Only finds exact text matches:
grep "error handling"  # Only finds this exact phrase
```

## Advanced Usage

### Distance Thresholds

Lower distances indicate higher similarity:

```bash
# Very strict matching (distance < 0.1)
search "API documentation" --threshold 0.1 docs/

# Moderate matching (distance < 0.3)  
search "user authentication" --threshold 0.3 src/

# Loose matching (distance < 0.5)
search "data processing" --threshold 0.5 *.py
```

### Context Control

```bash
# Minimal context (1 line before/after)
search "function definition" --context 1 src/

# Extended context (10 lines before/after)
search "configuration setup" --context 10 config/

# No additional context (just the matching line)
search "import statement" --context 0 *.py
```

## Model Information

The search tool uses:
- **Model**: `minishlab/potion-multilingual-128M` for effecient static embeddings
- **Similarity**: Cosine distance via simsimd
- **Languages**: Multilingual support (English, Spanish, French, German, etc.)
