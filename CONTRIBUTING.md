# Contributing to SemTools

We welcome contributions to SemTools! This document provides guidelines for contributing to the project.

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Git
- For the parse tool: LlamaIndex Cloud API key (for testing)

### Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/semtools
   cd semtools
   ```

2. **Build the project**
   ```bash
   cargo build
   ```

3. **Run tests**
   ```bash
   cargo test
   ```

4. **Install for development**
   ```bash
   cargo install --path .
   ```

### Project Structure

```
semtools/
├── crates/
│   ├── parse/              # Document parsing tool
│   │   ├── src/
│   │   │   ├── main.rs     # CLI interface
│   │   │   └── llama_parse_backend.rs  # LlamaIndex API integration
│   │   └── Cargo.toml
│   ├── search/             # Semantic search tool  
│   │   ├── src/
│   │   │   └── main.rs     # CLI interface and search logic
│   │   └── Cargo.toml
│   └── common/             # Shared utilities (future)
├── docs/                   # Documentation
├── tests/                  # Integration tests
└── Cargo.toml             # Workspace configuration
```

## How to Contribute

### Reporting Issues

When reporting issues, please include:

- **Clear title and description**
- **Steps to reproduce** the issue
- **Expected vs actual behavior**
- **Environment details** (OS, Rust version, etc.)
- **Sample files** if relevant (for parsing issues)

### Suggesting Features

For feature requests:

1. **Check existing issues** to avoid duplicates
2. **Describe the use case** and problem being solved
3. **Provide examples** of how the feature would be used
4. **Consider alternatives** and why this approach is best

### Pull Requests

1. **Fork the repository** and create a feature branch
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following our coding standards
3. **Add tests** for new functionality
4. **Update documentation** if needed
5. **Ensure tests pass**
   ```bash
   cargo test
   cargo clippy
   cargo fmt
   ```

6. **Submit a pull request** with:
   - Clear title and description
   - Reference to related issues
   - Summary of changes made

## Coding Standards

### Rust Guidelines

- **Follow Rust conventions** (use `cargo fmt` and `cargo clippy`)
- **Write clear, self-documenting code**
- **Use meaningful variable and function names**
- **Add doc comments** for public APIs
- **Handle errors appropriately** (use `anyhow::Result`)

### Code Style

```rust
// Good: Clear function with documentation
/// Searches for semantically similar text in the given documents
pub fn search_documents(
    query: &str,
    documents: &[Document],
    threshold: f64,
) -> Result<Vec<SearchResult>> {
    // Implementation
}

// Good: Error handling
let config = LlamaParseConfig::from_config_file(&config_path)
    .context("Failed to load configuration")?;

// Good: Clear variable names
let similarity_threshold = args.threshold.unwrap_or(0.3);
let context_lines = args.context;
```

### CLI Design Principles

- **Follow Unix philosophy**: Do one thing well
- **Support pipelines**: Read from stdin, write to stdout (`println!` vs. `eprintln!` !)
- **Provide helpful error messages**
- **Use consistent argument naming**
- **Include examples in help text**

### Testing

#### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_calculation() {
        // Test implementation
    }
}
```

#### Integration Tests
```bash
# Add integration tests in tests/ directory
tests/
├── parse_integration.rs
└── search_integration.rs
```

### Documentation

- **Update README files** for user-facing changes
- **Add inline comments** for complex logic
- **Include usage examples** in documentation
- **Update CLI help text** when adding options

## Development Workflow

### Adding a New Feature

1. **Create an issue** to discuss the feature
2. **Design the API** and get feedback
3. **Implement the feature** with tests
4. **Update documentation**
5. **Submit a pull request**

### Bug Fixes

1. **Reproduce the bug** and add a test case
2. **Fix the issue** with minimal changes
3. **Verify the fix** doesn't break existing functionality
4. **Update tests** if needed

### Performance Improvements

1. **Benchmark current performance** 
2. **Profile to identify bottlenecks**
3. **Implement improvements** with measurements
4. **Ensure no regressions** in functionality

## Specific Areas for Contribution

### Parse Tool

- **Add new backends** (local parsing, other APIs)
- **Improve error handling** and retry logic
- **Add more configuration options**
- **Optimize caching strategy**

### Search Tool

- **Support different embedding models**
- **Add more similarity metrics**
- **Improve result ranking**
- **Add search result highlighting**

### General Improvements

- **Better error messages** and help text
- **Performance optimizations**
- **Additional output formats** (JSON, CSV)
- **Integration with more tools**

## Code Review Guidelines

### For Contributors

- **Keep PRs focused** on a single feature/fix
- **Write clear commit messages**
- **Respond to feedback** constructively
- **Update based on review comments**

### For Reviewers

- **Be constructive** and helpful
- **Focus on code quality** and correctness
- **Consider maintainability** and performance
- **Suggest improvements** rather than just pointing out issues

## Release Process

1. **Update version numbers** in Cargo.toml files
2. **Update CHANGELOG.md** with new features and fixes
3. **Create a release tag**
4. **Build and test** release binaries
5. **Publish to crates.io** (maintainers only)

## Getting Help

- **Open an issue** for bugs or questions
- **Check existing documentation** and issues first
- **Provide context** and examples when asking for help

## License

By contributing to SemTools, you agree that your contributions will be licensed under the MIT License.

## Recognition

Contributors will be acknowledged in:
- **CHANGELOG.md** for significant contributions
- **README.md** contributors section
- **Release notes** for major features

Thank you for contributing to SemTools! 🎉 