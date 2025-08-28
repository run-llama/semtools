# Using Semtools with Coding Agents

Many coding agents (Claude-Code, Cursor, Gemini CLI, etc.) can be used to automate not only code generation, but also general tasks that involve your files.

`Semtools` can be used directly with these agents to augment their capabilities and allow for:

- parsing files that are not supported by the agent (PDFs, Word docs, etc.)
- applying semantic search to your files to find relevant information
- chaining parse and search commands, along with other CLI commands, to create powerful pipelines

All it takes is:

1. Install `semtools`: `cargo install semtools`
2. Ensure your parse config or `LLAMA_CLOUD_API_KEY` is set
3. Seed your `AGENTS.md` file (or `CLAUDE.md`, etc.) with knowledge about semtools
4. Use your agent!

Let's walk through an example of using `semtools` with Claude-Code.

## 1. Install `semtools`

```bash
cargo install semtools
```

## 2. Ensure your parse config or `LLAMA_CLOUD_API_KEY` is set

```bash
export LLAMA_CLOUD_API_KEY=your_api_key
```

## 3. Seed your `CLAUDE.md` file (or `AGENTS.md`, etc.) with knowledge about semtools

In your `CLAUDE.md` file, add something resembling the [example CLAUDE.md file](example_CLAUDE.md).

## 4. Use your agent!

Now, you can use your agent to automate tasks that involve your files.

In this example, I have a folder of 900+ papers (as PDFs) from an AI conference.

First, I parse the PDFs (which caches them to disk at `~/.parse`):

```bash
parse ./papers
```

Next, I invoke `claude` and ask it to summarize its takes on aspects of the papers. Using `parse`, `search`, and other common CLI commands, Claude is able to gather specific information from all of the papers.

Below is the full trace from Claude-Code, which demonstrates it's ability to use `semtools` like `parse` and `search` to gather information from your files.

```bash
╭────────────────────────────────────────────────────────────────╮
│ ✻ Welcome to Claude Code!                                      │
│                                                                │
│   /help for help, /status for your current setup               │
│                                                                │
│   cwd: /Users/loganmarkewich/filesystem-index/acl_papers_2023  │
╰────────────────────────────────────────────────────────────────╯

> Currently you are in a directory full of PDF files containing papers from ACL 2023. Can you summarize the findings from these papers that 
  discuss LLMs and evals?

⏺ I'll help you summarize findings from ACL 2023 papers about LLMs and evaluations. Let me first explore the directory structure and then
  search for relevant papers.

⏺ Update Todos
  ⎿  ☐ Explore directory structure to understand the PDF files
     ☐ Parse PDF files to make them searchable
     ☐ Search for papers discussing LLMs and evaluations
     ☐ Analyze and summarize key findings

⏺ List(~/filesystem-index/acl_papers_2023)
  ⎿  Listed 914 paths (ctrl+r to expand)

⏺ Update Todos
  ⎿  ☒ Explore directory structure to understand the PDF files
     ☐ Parse PDF files to make them searchable
     ☐ Search for papers discussing LLMs and evaluations
     ☐ Analyze and summarize key findings

⏺ I can see there are 910 ACL 2023 papers in PDF format. Now I'll parse all these PDFs to make them searchable, then look for papers
  discussing LLMs and evaluations.

⏺ Bash(parse *.pdf)
  ⎿  /Users/loganmarkewich/.parse/2023.acl-long.0.pdf.md 
     /Users/loganmarkewich/.parse/2023.acl-long.1.pdf.md
     /Users/loganmarkewich/.parse/2023.acl-long.10.pdf.md
     … +556 lines (ctrl+r to expand)

⏺ Update Todos
  ⎿  ☒ Explore directory structure to understand the PDF files
     ☒ Parse PDF files to make them searchable
     ☐ Search for papers discussing LLMs and evaluations
     ☐ Analyze and summarize key findings

⏺ Great! Now I have parsed all the PDFs. Let me search for papers related to LLMs and evaluations using the search tool.

⏺ Bash(parse *.pdf | xargs cat | search "large language model, evaluation, assessment, benchmark" --n-lines 5 --max-distance 0.4)
  ⎿  <stdin>:447094::447105 (0.16900031210055844)
     447095: for Computational Linguistics.
     447096:
     … +415 lines (ctrl+r to expand)

⏺ Now let me search for more specific terms to get a broader view of LLM evaluation research:

⏺ Bash(parse *.pdf | xargs cat | search "GPT, BERT, language model evaluation, performance evaluation, model comparison" --n-lines 5
      --max-distance 0.35)
  ⎿  <stdin>:244769::244780 (0.2719643874147169)                                                             
     244770: <td>
     244771: - Reference-free and reference-based human evaluation results have a near-zero correlation.<br/>
     … +630 lines (ctrl+r to expand)

⏺ Now let me search for specific types of evaluation studies and findings:

⏺ Bash(parse *.pdf | xargs cat | search "robustness, bias, fairness, hallucination, reliability" --n-lines 4 --max-distance 0.3)
  ⎿  Using cached result for: 2023.acl-long.0.pdf 
     Using cached result for: 2023.acl-long.1.pdf
     Using cached result for: 2023.acl-long.10.pdf
     … +639 lines (ctrl+r to expand)

⏺ Now let me focus on a few specific papers to get key titles and detailed findings:

⏺ Update Todos
  ⎿  ☒ Explore directory structure to understand the PDF files
     ☒ Parse PDF files to make them searchable
     ☒ Search for papers discussing LLMs and evaluations
     ☐ Analyze and summarize key findings

⏺ Let me search for paper titles and abstracts to get a better understanding of the specific papers discussing LLM evaluation:

⏺ Bash(parse *.pdf | xargs cat | search "Abstract" --n-lines 10 | grep -A 10 -B 2 -i "large language model\|evaluation\|benchmark\|GPT\|BERT")
  ⎿  <stdin>:94456::94477 (0.00000000009809730805202874)                    
     94457:   Plan-and-Solve Prompting: Improving Zero-Shot Chain-of-Thought
     94458:                 Reasoning by Large Language Models
     … +628 lines (ctrl+r to expand)

⏺ Update Todos
  ⎿  ☒ Explore directory structure to understand the PDF files
     ☒ Parse PDF files to make them searchable
     ☒ Search for papers discussing LLMs and evaluations
     ☒ Analyze and summarize key findings

⏺ Based on my comprehensive search through all 910 ACL 2023 papers, here are the key findings about LLMs and evaluations:

  Summary of LLM Evaluation Research from ACL 2023

  Major Benchmark and Evaluation Studies

  1. New Benchmark Creation
    - bgGLUE: A Bulgarian language understanding evaluation benchmark with 9 tasks
    - MULTISIM: Multilingual text simplification benchmark covering 12 languages with 1.7M sentence pairs
    - ROBUT: First diagnostic evaluation benchmark for Table QA robustness
    - CREPE: Benchmark for question-answering with false presuppositions
    - LMentry: Tests basic language abilities designed to complement standard LLM evaluations
  2. Cross-lingual and Multilingual Evaluation
    - XTREME-R: Multitask benchmark across 50 languages
    - XGLUE: Text classification benchmark covering 11 tasks in 19 languages
    - XTREME-UP: Evaluates 88 under-represented languages on 9 tasks
    - XSEMPLR: Cross-lingual semantic parsing evaluation with 8 meaning representations

  Key Evaluation Findings

  1. LLM Performance Insights
    - Zero-shot large language models (GPT-3) perform better under reference-free evaluation but worse under reference-based evaluation
  compared to smaller fine-tuned models
    - T5-LM finetuned on synthetic data (Unnatural Instructions) outperformed several strong instruction-tuned baselines including T0++ and
  Tk-Instruct
    - Context-aware translation models only perform significantly better than context-agnostic baselines for certain discourse phenomena
  2. Evaluation Methodology Issues
    - Benchmark Concurrence: Many NLP benchmarks have high "concurrence" - they rank modeling approaches similarly, suggesting redundancy in
  evaluation
    - Human Evaluation Problems: Reference-free and reference-based human evaluation results have near-zero correlation
    - Metric Reliability: Automatic metrics show moderate summary-level correlation but vary greatly under different evaluation protocols
  3. Model Robustness and Limitations
    - State-of-the-art models show significant performance drops on robustness benchmarks like ROBUT
    - LLMs struggle with false presupposition identification and correction
    - Massive language models like GPT-3 often contain factually false information and rarely explicitly identify false presuppositions
  4. Evaluation Best Practices Recommendations
    - Build smaller but more challenging benchmarks rather than larger ones
    - Seek diversity in benchmark construction to push research in new directions
    - Use more fine-grained evaluation methods that reveal specific model weaknesses
    - Consider extra-linguistic context when evaluating model capabilities

  Novel Evaluation Approaches

  1. Adversarial Training Evaluation: LETA framework uses LLMs to generate adversarial training examples to test model robustness
  2. Breakdown Detection: Evaluating MT metrics for their ability to detect translation failures that could cause downstream task breakdowns
  3. Dialect Robustness: NANO method provides size-efficient improvements for dialect robustness while maintaining standard benchmark
  performance
  4. Vision-Language Evaluation: New approaches combining language modeling with object localization to assess grounding capabilities

  Implications for the Field

  The research reveals that evaluation of LLMs in 2023 was transitioning from simple accuracy metrics to more nuanced, robust, and 
  comprehensive assessment methods. There's growing recognition that traditional benchmarks may be insufficient for evaluating the complex
  capabilities and failure modes of large language models, leading to increased focus on robustness, multilingual capabilities, and real-world
   applicability.
```

This run cost $0.68 and took about 4 minutes. Not bad for researching across 900+ papers! 

## Tips

- Your `CLAUDE.md` / `AGENTS.md` / etc. is critical to ensure your agent knows about `semtools`. Giving it good examples of usage (maybe from this guide) is a good idea.
- Parsing files can take a while. I recommend using `parse` ahead of time if you intend to give your agent a lot of files to work with that need to be parsed. Results will be cached to disk at `~/.parse`.
- Giving your agent any context on the files it has access to will also help it understand how to help you with them.
