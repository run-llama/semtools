# Arxiv Dataset Agent

You have access to many folders containing papers from ArXiv, organized in different ways.

## File Structure

```
./
├── by_author/
│   ├── Aadhrik_Kulia
│   │   ├── 2507.22047v1_fulltext.txt
│   │   └── ...
│   └── ...
├── by_category/
│   ├── cs.AI
│   │   ├── 2505.20278v1_fulltext.txt
│   │   └── ...
│   └── ...
├── by_date/
│   ├── 2025-05
│   │   ├── 2505.20277v2_fulltext.txt
│   │   └── ...
│   └── ...
├── full_text/
│   ├── 2505.20277v2.txt
|   └── ...
```

## File Sample

```
TITLE: Quantization vs Pruning: Insights from the Strong Lottery Ticket Hypothesis

Quantization vs Pruning: Insights from the Strong Lottery Ticket Hypothesis

Aakash Kumar 
Department of Physical Sciences, IISER Kolkata, 
West Bengal, India 741246 
ak20ms209@iiserkol.ac.in
&Emanuele Natale 
Université Coté d’Azur,
CNRS, Inria, I3S, France
emanuele.natale@univ-cotedazur.fr

Abstract
Quantization is an essential technique for making neural networks more efficient, yet our theoretical understanding of it remains limited. Previous works demonstrated that extremely low-precision networks, such as binary networks, can be constructed by pruning large, randomly-initialized networks, and showed that the ratio between the size of the original and the pruned networks is at most polylogarithmic.
The specific pruning method they employed inspired a line of theoretical work known as the Strong Lottery Ticket Hypothesis (SLTH), which leverages insights from the Random Subset Sum Problem. However, these results primarily address the continuous setting and cannot be applied to extend SLTH results to the quantized setting.
In this work, we build on foundational results by Borgs et al. on the Number Partitioning Problem to derive new theoretical results for the Random Subset Sum Problem in a quantized setting.
Using these results, we then extend the SLTH framework to finite-precision networks. While prior work on SLTH showed that pruning allows approximation of a certain class of neural networks, we demonstrate that, in the quantized setting, the analogous class of target discrete neural networks can be represented exactly, and we prove optimal bounds on the necessary overparameterization of the initial network as a function of the precision of the target network.

1 Introduction

Deep neural networks (DNNs) have become ubiquitous in modern machine-learning systems, yet their ever-growing size quickly collides with the energy, memory, and latency constraints of real-world hardware. Quantization—representing weights with a small number of bits—is arguably the most hardware-friendly compression technique, and recent empirical work shows that aggressive quantization can preserve accuracy even down to the few bits regime. Unfortunately, our theoretical understanding of why and when such extreme precision reduction is possible still lags far behind in practice.
...
```

## Executing Bash Commands

When executing bash commands, you have a helpful new CLI command: `search` -- performs a search using static embeddings on either stdin or a list of files (very similar to grep). Works best with keyword based search queries.

With this command, you can ensure that you can search large amounts of files efficiently. You can think of it like a fuzzy grep or fuzzy BM25. 

It's useful to think carefully about when you need the exact match power of grep and when you you can use the fuzzy power of this `search` command. Usually there are some patterns that you can follow to help you decide:

- Will grep output a lot of results? Use `search` on the output of grep to help filter 
- Will search output a lot of results? Use grep to filter the results of search
- Looking for exact matches or a known pattern? Use grep
- Looking for a fuzzy match or a pattern that you don't know? Use `search`
- Interacting with a huge amount of files? Sometimes things are a lot faster if you can use grep first with a slightly generic pattern, and then use `search` to semantically filter/find the results you care about

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

## Common Usage Patterns for `search`

```bash
# Search with custom context and thresholds or distance thresholds
search "machine learning" *.txt --n-lines 6 --max-distance 0.4 --ignore-case

# Search multiple files directly
search "error handling" src/*.rs --top-k 5 --n-lines 5 --ignore-case

# Combine with grep for exact-match pre-filtering and distance thresholding
grep -i "error" some_folder/*.txt | search "network error" --max-distance 0.3 --n-lines 7

# grep after search
search "network error" some_folder/*.txt --n-lines 10 | grep -i "error"

# Pipeline with content search (note the 'cat')
find . -name "*.md" | xargs search "installation" --n-lines 10 --max-distance 0.5
```
