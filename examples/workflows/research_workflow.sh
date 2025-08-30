#!/bin/bash

# Research Paper Analysis Workflow with LMStudio Backend
# This script demonstrates how to process research papers and extract insights

set -e  # Exit on any error

# Configuration
PAPERS_DIR="research_papers"
OUTPUT_DIR="analysis_results"
CONFIG_FILE="$HOME/.lmstudio_parse_config.json"

echo "🔬 Research Paper Analysis Workflow"
echo "===================================="

# Check if LMStudio server is running
echo "📡 Checking LMStudio server..."
if ! curl -s http://localhost:1234/v1/models > /dev/null; then
    echo "❌ LMStudio server not accessible. Please start LMStudio and load a model."
    exit 1
fi

echo "✅ LMStudio server is running"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if papers directory exists
if [ ! -d "$PAPERS_DIR" ]; then
    echo "📁 Creating example papers directory..."
    mkdir -p "$PAPERS_DIR"
    echo "Please add your research papers (PDF format) to the $PAPERS_DIR directory"
    echo "Example: cp ~/Downloads/*.pdf $PAPERS_DIR/"
    exit 1
fi

# Count papers
PAPER_COUNT=$(find "$PAPERS_DIR" -name "*.pdf" | wc -l | tr -d ' ')
if [ "$PAPER_COUNT" -eq 0 ]; then
    echo "❌ No PDF files found in $PAPERS_DIR"
    echo "Please add research papers to process"
    exit 1
fi

echo "📚 Found $PAPER_COUNT papers to process"

# Parse all papers with LMStudio backend
echo "🤖 Parsing papers with LMStudio..."
parse "$PAPERS_DIR"/*.pdf --backend lmstudio -v

echo "✅ Parsing complete!"

# Define research topics to search for
TOPICS=(
    "methodology"
    "results"
    "conclusion"
    "neural networks"
    "machine learning"
    "artificial intelligence"
    "deep learning"
    "data analysis"
    "experimental design"
    "statistical significance"
)

echo "🔍 Extracting insights for key topics..."

# Search for each topic and save results
for topic in "${TOPICS[@]}"; do
    echo "  📋 Searching for: $topic"
    search "$topic" ~/.parse/lmstudio/*.md --max-distance 0.4 > "$OUTPUT_DIR/${topic//, /_}.txt" 2>/dev/null || true
done

# Create summary report
echo "📊 Generating summary report..."
cat > "$OUTPUT_DIR/research_summary.md" << EOF
# Research Paper Analysis Summary

Generated on: $(date)
Papers processed: $PAPER_COUNT

## Topics Analyzed

EOF

# Add topic summaries
for topic in "${TOPICS[@]}"; do
    file="$OUTPUT_DIR/${topic//, /_}.txt"
    if [ -f "$file" ] && [ -s "$file" ]; then
        count=$(wc -l < "$file")
        echo "- **$topic**: $count matches found" >> "$OUTPUT_DIR/research_summary.md"
    fi
done

cat >> "$OUTPUT_DIR/research_summary.md" << EOF

## Files Generated

- \`research_summary.md\` - This summary
EOF

for topic in "${TOPICS[@]}"; do
    file="${topic//, /_}.txt"
    if [ -f "$OUTPUT_DIR/$file" ] && [ -s "$OUTPUT_DIR/$file" ]; then
        echo "- \`$file\` - Excerpts about $topic" >> "$OUTPUT_DIR/research_summary.md"
    fi
done

cat >> "$OUTPUT_DIR/research_summary.md" << EOF

## Usage

View specific topics:
\`\`\`bash
less $OUTPUT_DIR/methodology.txt
\`\`\`

Search for custom terms:
\`\`\`bash
search "your custom term" ~/.parse/lmstudio/*.md
\`\`\`

## Parsed Files Location

All parsed papers are available in markdown format at:
\`~/.parse/lmstudio/\`

EOF

echo "🎉 Analysis complete!"
echo ""
echo "📋 Results saved to: $OUTPUT_DIR/"
echo "📄 Summary: $OUTPUT_DIR/research_summary.md"
echo ""
echo "💡 Next steps:"
echo "   - Review the summary: cat $OUTPUT_DIR/research_summary.md"
echo "   - Explore specific topics: ls $OUTPUT_DIR/*.txt"
echo "   - Run custom searches: search 'your term' ~/.parse/lmstudio/*.md"
echo ""
echo "✨ Happy researching!"