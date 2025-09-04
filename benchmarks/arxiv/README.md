# ArXiv Benchmark

This directory contains the code and data for the ArXiv benchmark for SemTools.

Using [`download_arxiv_files.py`](./download_arxiv_files.py), we downloaded 1000 papers from ArXiv and organized them into a directory structure.

You can download the full dataset [here](https://1drv.ms/u/c/09c2f12f30f0f39b/EaSMcXdzxYZMuTI4HyTaGs8BT9GaHhj6GiZQORy2AfSIAA?e=vCIZBg) instead of running the script.

The dataset is organized into the following directories:

```
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

## Running the Benchmark

The benchmark was originally run between September 1st -> 3rd, 2025, using v1.0.90 of cloud-code and v1.2.0 of SemTools.

To run the benchmark, simply set the `CLAUDE.md` file you want to use in the `arxiv_dataset_1000_papers` directory (either plain or search), and then prompt claude-code with the questions in [questions.txt](./questions.txt). Each question is asked in a new chat, and the responses are copy-pasted to the `answers` directory.

## Existing Results

You can find the full results in [`answers/`](./answers/) folder. Each question has two responses: `with search` and `without search`, representing which `CLAUDE.md` file was used.
