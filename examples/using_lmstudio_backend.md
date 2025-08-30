# Using Semtools with LMStudio Backend

This guide demonstrates how to use the LMStudio backend for **completely local, private document parsing** with semtools.

## Why Use LMStudio Backend?

✅ **Complete Privacy** - Documents never leave your machine  
✅ **No API Costs** - Free after initial setup  
✅ **Offline Capable** - Works without internet  
✅ **Customizable Models** - Choose your preferred LLM  
✅ **No Rate Limits** - Process as many documents as you want  

## Quick Start

### 1. Install LMStudio

1. Download [LMStudio](https://lmstudio.ai) for your platform
2. Install and launch the application

### 2. Download a Model

Recommended models for document parsing:

**For General Documents:**
- `Llama-3.2-3B-Instruct` (fast, good quality)
- `Mistral-7B-Instruct-v0.3` (excellent balance)
- `Phi-3.5-mini-instruct` (very fast, lightweight)

**For Technical Documents:**
- `CodeLlama-13B-Instruct` (better with code/technical content)
- `Qwen2.5-7B-Instruct` (good with structured documents)

**For Large Documents:**
- Any model with 32k+ context (look for "32k" or "128k" in model name)

### 3. Start LMStudio Server

1. In LMStudio, go to the **"Local Server"** tab
2. Select your downloaded model
3. Click **"Start Server"**
4. Note the server URL (usually `http://localhost:1234`)

### 4. Configure Semtools

Create `~/.lmstudio_parse_config.json`:

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
  "num_ongoing_requests": 3
}
```

**⚠️ Important:** Set the `model` field to match the exact model name shown in LMStudio.

**Check your model name:**
```bash
curl http://localhost:1234/v1/models
```

### 5. Install Semtools

```bash
cargo install semtools
```

## Basic Usage Examples

### Parse a Single Document

```bash
# Parse a PDF with LMStudio
parse document.pdf --backend lmstudio

# Parse with verbose output to see progress
parse document.pdf --backend lmstudio -v
```

### Parse Multiple Documents

```bash
# Parse all PDFs in a directory
parse docs/*.pdf --backend lmstudio

# Parse mixed document types
parse *.pdf *.docx *.pptx --backend lmstudio
```

### Complete Parse + Search Workflow

```bash
# Parse documents and search for specific content
parse reports/*.pdf --backend lmstudio | xargs -n 1 search "quarterly results"

# Search with distance threshold for more precise results
parse documents/*.docx --backend lmstudio | xargs -n 1 search "machine learning" --max-distance 0.4

# Chain with other Unix tools
parse *.pdf --backend lmstudio | xargs cat | grep -i "revenue" | search "financial projections"
```

## Advanced Configuration

### Performance Tuning

For **faster processing** (multiple smaller documents):
```json
{
  "num_ongoing_requests": 5,
  "chunk_size": 2000,
  "temperature": 0.1
}
```

For **better quality** (complex documents):
```json
{
  "temperature": 0.5,
  "max_tokens": 8192,
  "chunk_size": 4000,
  "chunk_overlap": 400
}
```

For **large documents**:
```json
{
  "chunk_size": 6000,
  "chunk_overlap": 500,
  "num_ongoing_requests": 2
}
```

### Custom Model Settings

Different models may need different settings:

**For Mistral models:**
```json
{
  "model": "mistral-7b-instruct-v0.3",
  "temperature": 0.2,
  "max_tokens": 4096
}
```

**For CodeLlama (technical docs):**
```json
{
  "model": "codellama-13b-instruct",
  "temperature": 0.1,
  "max_tokens": 2048
}
```

## Real-World Workflows

### 1. Research Paper Analysis

```bash
# Parse academic papers and search for specific topics
mkdir parsed_papers
parse research_papers/*.pdf --backend lmstudio
search "neural networks" ~/.parse/lmstudio/*.md --max-distance 0.3 > neural_network_findings.txt
search "methodology" ~/.parse/lmstudio/*.md --max-distance 0.4 > methodology_sections.txt
```

### 2. Legal Document Review

```bash
# Parse contracts and search for key terms
parse contracts/*.pdf --backend lmstudio -v
search "liability" ~/.parse/lmstudio/*.md | grep -A 3 -B 3 "limitation"
search "termination clause" ~/.parse/lmstudio/*.md --max-distance 0.2
```

### 3. Technical Documentation Processing

```bash
# Parse technical manuals and create searchable knowledge base
parse manuals/*.pdf technical_specs/*.docx --backend lmstudio
echo "API endpoints" > search_terms.txt
echo "configuration options" >> search_terms.txt
echo "troubleshooting steps" >> search_terms.txt

while read term; do
  echo "=== Searching for: $term ===" >> knowledge_base.txt
  search "$term" ~/.parse/lmstudio/*.md --max-distance 0.3 >> knowledge_base.txt
  echo "" >> knowledge_base.txt
done < search_terms.txt
```

### 4. Meeting Notes and Reports

```bash
# Parse meeting minutes and quarterly reports
parse meetings/*.docx reports/*.pdf --backend lmstudio

# Extract action items and decisions
search "action item" ~/.parse/lmstudio/*.md > action_items.txt
search "decision" ~/.parse/lmstudio/*.md --max-distance 0.4 > decisions.txt
search "follow up" ~/.parse/lmstudio/*.md >> action_items.txt
```

## Troubleshooting

### LMStudio Connection Issues

**Check if server is running:**
```bash
curl http://localhost:1234/v1/models
```

**Common fixes:**
- Restart LMStudio server
- Check firewall settings
- Verify model is loaded in LMStudio
- Ensure correct port (default: 1234)

### Parsing Quality Issues

**For better formatting:**
- Lower temperature (0.1-0.2)
- Increase max_tokens
- Try different model

**For faster processing:**
- Smaller chunk_size (1500-2000)  
- Higher temperature (0.5)
- Reduce num_ongoing_requests

**For large documents:**
- Increase chunk_overlap (300-500)
- Use model with larger context window
- Reduce num_ongoing_requests to avoid memory issues

### Performance Optimization

**Speed up processing:**
```json
{
  "num_ongoing_requests": 6,
  "chunk_size": 1500,
  "retry_delay_ms": 500,
  "temperature": 0.1
}
```

**Improve quality:**
```json
{
  "num_ongoing_requests": 2,
  "chunk_size": 4000,
  "chunk_overlap": 400,
  "temperature": 0.3,
  "max_tokens": 6144
}
```

## Model Recommendations by Use Case

### Business Documents
- **Llama-3.2-3B-Instruct**: Fast, good for standard business docs
- **Qwen2.5-7B-Instruct**: Excellent with tables and structured content

### Academic Papers  
- **Mistral-7B-Instruct**: Great comprehension, handles complex language
- **Mixtral-8x7B**: Best quality, slower but worth it for important docs

### Technical Documentation
- **CodeLlama-13B**: Understanding of code and technical concepts
- **DeepSeek-Coder-6.7B**: Excellent with API docs and technical specs

### Legal Documents
- **Llama-3.1-8B-Instruct**: Good reasoning, handles complex language
- **Claude-3-Haiku** (if available): Excellent with formal language

## Privacy and Security Benefits

✅ **Data Never Leaves Your Machine** - Complete privacy  
✅ **No Cloud Storage** - Documents processed locally  
✅ **No API Logs** - No third-party tracking  
✅ **Offline Processing** - Works without internet  
✅ **Custom Models** - Use specialized or fine-tuned models  
✅ **No Usage Limits** - Process unlimited documents  

This makes LMStudio backend perfect for:
- Confidential business documents
- Legal and compliance materials  
- Personal documents
- Regulated industry content
- Sensitive research materials

## Next Steps

1. **Try different models** to find what works best for your document types
2. **Experiment with settings** to balance speed vs. quality
3. **Create custom configs** for different document workflows  
4. **Combine with other tools** in your processing pipeline
5. **Set up batch processing scripts** for regular document workflows

Happy parsing! 🚀