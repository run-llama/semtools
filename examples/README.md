# Semtools Examples

This directory contains examples and workflows for using semtools effectively.

## 📚 Available Examples

### Core Usage Guides

- **[Using LMStudio Backend](using_lmstudio_backend.md)** - Complete guide to local, private document parsing
- **[Using with Coding Agents](use_with_coding_agents.md)** - Integration with Claude-Code and other AI assistants
- **[Using with MCP](use_with_mcp.md)** - Model Context Protocol integration


### Automated Workflows

- **[Research Workflow](workflows/research_workflow.sh)** - Analyze research papers automatically
- **[Knowledge Base Creator](workflows/document_knowledge_base.sh)** - Build searchable document collections

## 🚀 Quick Start

### 1. Choose Your Backend

**For privacy-focused users (recommended):**
```bash
# Install LMStudio, load a model, start server
# Create ~/.lmstudio_parse_config.json (see README for examples)
parse documents/*.pdf --backend lmstudio
```

**For cloud processing:**
```bash
export LLAMA_CLOUD_API_KEY=your_key
parse documents/*.pdf --backend llama-parse
```

### 2. Run Example Workflows

```bash
# Set up automated research workflow
chmod +x examples/workflows/research_workflow.sh
./examples/workflows/research_workflow.sh

# Create a searchable knowledge base
chmod +x examples/workflows/document_knowledge_base.sh
./examples/workflows/document_knowledge_base.sh
```

### 3. Common Usage Patterns

```bash
# Parse and search in one command
parse *.pdf --backend lmstudio | xargs -n 1 search "your topic"

# Build knowledge base from mixed document types
parse documents/*.pdf documents/*.docx --backend lmstudio
search "important topic" ~/.parse/lmstudio/*.md

# Use with other Unix tools
parse reports/*.pdf --backend lmstudio | xargs cat | grep -i "revenue" | search "quarterly"
```

## 📖 Usage Recommendations

### For Different Use Cases

**Research & Academia:**
- Use higher quality settings (see README config examples)
- Try the `workflows/research_workflow.sh` for automated analysis
- Use higher chunk overlap (400+) for complex papers

**Business Documents:**
- Use fast processing settings for quick turnaround
- Focus on structured search terms
- Combine with grep for exact matches

**Technical Documentation:**
- Use code-focused models and settings
- Increase context lines (`-n 5`) for code examples
- Use stricter distance thresholds (`--max-distance 0.3`)

**Personal Documents:**
- LMStudio backend for complete privacy
- Custom configs for your specific needs
- Build persistent knowledge bases with workflow scripts

## 🔧 Configuration Tips

### Model Recommendations by Task

| Task | Recommended Model | Settings Focus |
|------|------------------|----------------|
| General documents | `llama-3.2-3b-instruct` | Speed optimized |
| Research papers | `mistral-7b-instruct-v0.3` | Quality optimized |
| Technical docs | `codellama-13b-instruct` | Technical content |
| Legal documents | `llama-3.1-8b-instruct` | Quality optimized |

### Performance Tuning

**For speed:**
- Increase `num_ongoing_requests` (4-6)
- Reduce `chunk_size` (1500-2000)
- Lower `temperature` (0.1)

**For quality:**
- Increase `chunk_overlap` (300-500) 
- Higher `max_tokens` (4096+)
- More precise `temperature` (0.3)

## 🆘 Troubleshooting

**LMStudio Issues:**
```bash
# Check if server is running
curl http://localhost:1234/v1/models

# Test with simple document
echo "test content" > test.doc
parse test.doc --backend lmstudio -v
```

**Search Quality:**
```bash
# Try different distance thresholds
search "topic" file.md --max-distance 0.2  # Strict
search "topic" file.md --max-distance 0.6  # Loose

# Increase context for better understanding
search "topic" file.md -n 5
```

**Performance:**
- Check your model size vs available RAM
- Reduce `num_ongoing_requests` if getting timeouts
- Use lighter models for batch processing

## 📞 Getting Help

- Check the main [README](../README.md) for installation and basic usage
- Browse individual example files for detailed instructions
- Test with the provided workflow scripts
- Join the community discussions for advanced usage

## 🤝 Contributing Examples

Have a useful workflow or configuration? Please contribute:

1. Create your example in the appropriate subdirectory
2. Follow the existing documentation style
3. Test your example thoroughly
4. Submit a pull request

Thank you for using semtools! 🚀