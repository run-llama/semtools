#!/bin/bash

# Document Knowledge Base Creation with LMStudio
# This script creates a searchable knowledge base from various document types

set -e

# Configuration
DOCS_DIR="documents"
KB_DIR="knowledge_base"
CONFIG_FILE="$HOME/.lmstudio_parse_config.json"

echo "📚 Document Knowledge Base Creator"
echo "=================================="

# Check LMStudio
echo "📡 Checking LMStudio server..."
if ! curl -s http://localhost:1234/v1/models > /dev/null; then
    echo "❌ LMStudio server not running. Please start LMStudio."
    exit 1
fi
echo "✅ LMStudio ready"

# Setup directories
mkdir -p "$KB_DIR"
if [ ! -d "$DOCS_DIR" ]; then
    mkdir -p "$DOCS_DIR"
    echo "📁 Created $DOCS_DIR - add your documents here"
    echo "Supported: PDF, DOCX, PPTX files"
    exit 1
fi

# Count documents
DOC_COUNT=$(find "$DOCS_DIR" -type f \( -name "*.pdf" -o -name "*.docx" -o -name "*.pptx" -o -name "*.doc" \) | wc -l | tr -d ' ')
if [ "$DOC_COUNT" -eq 0 ]; then
    echo "❌ No documents found in $DOCS_DIR"
    exit 1
fi

echo "📄 Processing $DOC_COUNT documents..."

# Parse all documents
echo "🤖 Parsing with LMStudio backend..."
find "$DOCS_DIR" -type f \( -name "*.pdf" -o -name "*.docx" -o -name "*.pptx" -o -name "*.doc" \) -print0 | \
xargs -0 parse --backend lmstudio -v

# Create knowledge base structure
cat > "$KB_DIR/README.md" << EOF
# Document Knowledge Base

Created: $(date)
Documents processed: $DOC_COUNT

## Quick Search Examples

\`\`\`bash
# Search for specific topics
search "project timeline" ~/.parse/lmstudio/*.md
search "budget analysis" ~/.parse/lmstudio/*.md --max-distance 0.3
search "meeting notes" ~/.parse/lmstudio/*.md -n 5

# Search with context
search "action items" ~/.parse/lmstudio/*.md -n 3 | grep -A2 -B2 "deadline"

# Combine searches
search "quarterly" ~/.parse/lmstudio/*.md | search "revenue"
\`\`\`

## Available Documents

EOF

# List processed documents
find ~/.parse/lmstudio/ -name "*.md" -type f | sort | while read file; do
    basename=$(basename "$file" .md)
    echo "- [$basename]($file)" >> "$KB_DIR/README.md"
done

cat >> "$KB_DIR/README.md" << EOF

## Search Categories

Common search patterns for your knowledge base:

### Business & Finance
\`\`\`bash
search "revenue" ~/.parse/lmstudio/*.md
search "budget" ~/.parse/lmstudio/*.md
search "quarterly results" ~/.parse/lmstudio/*.md
search "financial projections" ~/.parse/lmstudio/*.md
\`\`\`

### Projects & Planning
\`\`\`bash
search "timeline" ~/.parse/lmstudio/*.md
search "milestone" ~/.parse/lmstudio/*.md
search "deliverable" ~/.parse/lmstudio/*.md
search "project scope" ~/.parse/lmstudio/*.md
\`\`\`

### Meetings & Actions
\`\`\`bash
search "action item" ~/.parse/lmstudio/*.md
search "follow up" ~/.parse/lmstudio/*.md
search "decision" ~/.parse/lmstudio/*.md
search "next steps" ~/.parse/lmstudio/*.md
\`\`\`

### Technical & Research
\`\`\`bash
search "methodology" ~/.parse/lmstudio/*.md
search "implementation" ~/.parse/lmstudio/*.md
search "requirements" ~/.parse/lmstudio/*.md
search "architecture" ~/.parse/lmstudio/*.md
\`\`\`

## Advanced Usage

### Export search results
\`\`\`bash
search "important topic" ~/.parse/lmstudio/*.md > important_findings.txt
\`\`\`

### Chain searches
\`\`\`bash
search "AI" ~/.parse/lmstudio/*.md | search "machine learning"
\`\`\`

### Use with grep
\`\`\`bash
search "project" ~/.parse/lmstudio/*.md | grep -i "deadline"
\`\`\`

---

*Knowledge base created with semtools + LMStudio backend*
EOF

# Create quick search script
cat > "$KB_DIR/search.sh" << 'EOF'
#!/bin/bash
# Quick search script for your knowledge base

if [ $# -eq 0 ]; then
    echo "Usage: ./search.sh <search_term> [max_distance]"
    echo "Example: ./search.sh 'project timeline'"
    echo "Example: ./search.sh 'revenue' 0.3"
    exit 1
fi

TERM="$1"
DISTANCE="${2:-0.5}"

echo "🔍 Searching for: $TERM"
echo "📏 Max distance: $DISTANCE"
echo "================================"

search "$TERM" ~/.parse/lmstudio/*.md --max-distance "$DISTANCE"
EOF

chmod +x "$KB_DIR/search.sh"

echo "🎉 Knowledge base created!"
echo ""
echo "📍 Location: $KB_DIR/"
echo "📖 Guide: $KB_DIR/README.md"
echo "🔍 Quick search: $KB_DIR/search.sh"
echo ""
echo "💡 Try these commands:"
echo "   cat $KB_DIR/README.md"
echo "   $KB_DIR/search.sh 'your search term'"
echo "   search 'anything' ~/.parse/lmstudio/*.md"
echo ""
echo "✨ Your knowledge base is ready!"