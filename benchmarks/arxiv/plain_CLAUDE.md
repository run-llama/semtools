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
