# Using Semtools with MCP

While it might be tempting to translate `semtools` commands like `parse` and `search` into MCP servers, we've found that the best approach is giving your agent access to the terminal directly. While this can be a bit more complex (and has security implications!), it's the best way to ensure your agent can use `semtools` to its full potential.

If you want to setup a basic MCP server for `semtools`, you can use the follow the steps below.

## 1. Install `semtools`

```bash
cargo install semtools
```

## 2. Configure an MCP server with terminal access (unsafe!)

This is a simple approach with python to get started, but it's not recommended for production use.

**NOTE:** Ideally, you'd launch a docker container and mount the volumes you want to interact with, in addition to using proper user permissions and other runtime restrictions. Giving any agent or LLM unrestricted access to the terminal is not recommended.

We will be using `fastmcp` to create an MCP server that can execute bash commands. You can read more about fastmcp in [their documentation](https://gofastmcp.com/getting-started/quickstart).

```bash
pip install fastmcp
```

And configure a fastmcp server to execute bash commands in `server.py`:

```python
import subprocess
from fastmcp import FastMCP

mcp = FastMCP("My MCP Server")

@mcp.tool
def execute_bash(command: str) -> str:
    """Useful for executing bash commands on your machine."""
    return subprocess.run(command, shell=True, capture_output=True, text=True, timeout=60).stdout
```

And launch the server:

```bash
fastmcp server.py:mcp --transport streamable-http
```

## 3. Connect the server to your agent/LLM

Now, you can use your server as a tool in your agent/LLM.

While this can be done in a variety of ways, we'll use `llama-index` below:

```bash
pip install llama-index-llms-anthropic llama-index-tools-mcp
```

To help prompt the agent, we'll also seed the system prompt with our [example `CLAUDE.md` file](example_CLAUDE.md) so that it knows about `semtools`.

Here's an example where I prompt it to interact with a directory of 900+ PDF files from an AI conference. 

First, I parse the PDFs (which caches them to disk at `~/.parse`):

```bash
parse ./papers
```

Then I can write a script to call an agent to interact with the files and search for information:

```python
import asyncio
from llama_index.core.agent import FunctionAgent, ToolCall, ToolCallResult
from llama_index.core.workflow import Context
from llama_index.llms.anthropic import Anthropic
from llama_index.tools.mcp import BasicMCPClient, McpToolSpec


async def main():
    llm = Anthropic(
        model="claude-sonnet-4-0", 
        api_key="sk-...",
        max_tokens=8192,
    )
    tool_spec = McpToolSpec(client=BasicMCPClient("http://127.0.0.1:8000/mcp"))
    tools = await tool_spec.to_tool_list_async()

    system_prompt = "You are a helpful assistant that has access to execute bash commands on your machine."
    with open("CLAUDE.md", "r") as f:
        system_prompt += "\n\n" + f.read()
    
    agent = FunctionAgent(llm=llm, tools=tools, system_prompt=system_prompt)
    ctx = Context(agent )  # context to hold chat history if needed

    query = "Currently you are in a directory full of PDF files containing papers from ACL 2023. Can you summarize the findings from these papers that discuss LLMs and evals?"
    handler = agent.run(query, ctx=ctx)
    async for ev in handler.stream_events():
        if isinstance(ev, ToolCall):
            print(f"Calling tool {ev.tool_name}({ev.tool_kwargs})")
        # elif isinstance(ev, ToolCallResult):
        #     print(f"Tool call result: {ev.tool_output}")

    response = await handler
    print(response)

if __name__ == "__main__":
    asyncio.run(main())
```

Which outputs:

```bash
$ python ./agent.py
Calling tool execute_bash({'command': 'find . -name "*.pdf" | head -10'})
Calling tool execute_bash({'command': 'find . -name "*.pdf" | wc -l'})
Calling tool execute_bash({'command': 'find . -name "*.pdf" | xargs parse'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "large language model, LLM, evaluation, benchmark" --top-k 10 --n-lines 5'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "GPT, ChatGPT, instruction tuning, evaluation benchmark, model evaluation" --top-k 15 --n-lines 4'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "human evaluation, automatic evaluation, BLEU, ROUGE, BERTScore, evaluation metric" --top-k 10 --n-lines 4'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "scaling laws, emergent abilities, few-shot learning, in-context learning, prompt engineering" --top-k 10 --n-lines 4'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "title, abstract" --top-k 5 --n-lines 10 | head -50'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | xargs search "# " --top-k 20 --n-lines 2 | grep -E "(LLM|evaluation|benchmark|GPT|language model)" | head -20'})
Calling tool execute_bash({'command': 'find /Users/loganmarkewich/.parse -name "*.md" | head -5 | xargs -I {} sh -c \'echo "=== {} ===" && head -20 "{}"\''})
Based on my analysis of the ACL 2023 papers, I can now provide a comprehensive summary of the key findings about LLMs and evaluations:

# Summary of ACL 2023 Findings on LLMs and Evaluations

## Key Themes and Findings

### 1. **Scaling and Emergent Abilities**
- **Competence emerges at scale**: Research shows that model robustness to linguistic features improves significantly with scale. For example, studies found a 20-point improvement in average performance from smaller to larger PaLM models, with the strongest models (PaLM 2) reaching near-perfect performance on English and Indo-European languages.
- **In-context learning capabilities**: Multiple papers highlighted the "magical" ability of large language models to adapt to new tasks through just a few demonstration examples without gradient descent, representing a paradigm shift from traditional fine-tuning approaches.

### 2. **Evaluation Methodologies and Challenges**

#### **Human vs. Automatic Evaluation**
- **Correlation with expert judgment**: Studies found that GPT models (GPT-3.5, ChatGPT, GPT-4) correlated well with expert judgments, often outperforming traditional crowd-sourced annotations.
- **Limitations of traditional metrics**: Research revealed significant issues with standard evaluation metrics like BLEU, ROUGE, and BERTScore, particularly for generative tasks. New metrics like GAVA-Score showed better correlation with human evaluation of answer accuracy.
- **Human evaluation inconsistencies**: Papers identified major problems in benchmark evaluation, including different test sets for humans vs. systems, inconsistent metrics across tasks, and varying pay rates affecting annotation quality.

#### **Benchmark Limitations**
- **Cross-lingual evaluation gaps**: Studies showed that while models perform well on English, significant gaps remain for other languages, with some models passing less than 50% of tests in languages like Slovak, Swahili, and Arabic.
- **Multilingual benchmark development**: New benchmarks like XTREME-R (50 languages), XGLUE (19 languages), and XTREME-UP (88 under-represented languages) were developed to address evaluation gaps.

### 3. **LLM Capabilities and Limitations**

#### **Strengths**
- **Data annotation potential**: GPT-3 showed promise as a data annotator, with studies exploring its effectiveness compared to traditional annotation methods.
- **Mathematical reasoning**: Large language models demonstrated improved capabilities in solving mathematical reasoning problems and theorem proving.
- **Multilingual performance**: Advanced models like PaLM 2 achieved near-perfect performance on multiple language tasks.

#### **Limitations**
- **Factual accuracy issues**: Studies found that models like GPT-3 often generate information that is factually false and rarely explicitly identify false presuppositions.
- **Mental model inconsistencies**: Research revealed that even advanced models like ChatGPT lack coherent mental models of everyday things, leading to nonsensical outputs.
- **Zero-shot vs. fine-tuned performance**: Studies consistently found that zero-shot ChatGPT fails to outperform fine-tuned models on more than 77% of NLP tasks.

### 4. **Novel Evaluation Approaches**

#### **LLM-as-Evaluator**
- **Pairwise evaluation**: Research introduced methods like chatbot arenas where users provide pairwise evaluations of LLM responses, leading to Elo rating systems for model comparison.
- **Automated evaluation**: Studies developed fine-tuned models like PandaLM to compare LLM responses, achieving 94% accuracy compared to ChatGPT-based comparisons.

#### **Task-Specific Evaluation**
- **Instruction tuning assessment**: Papers evaluated models like FLAN-T5 and instruction-tuned variants, showing that automated data generation with pretrained LMs can be a viable alternative to human-curated data.
- **Temporal reasoning**: New benchmarks like TempReason were developed to evaluate models' understanding of temporal relationships and reasoning.

### 5. **Methodological Innovations**

#### **Few-Shot and In-Context Learning**
- **Meta-function approaches**: Research proposed treating PLMs as meta-functions that can inject in-context abilities for tasks like named entity recognition.
- **Prompt engineering**: Studies explored various prompting strategies, including chain-of-thought reasoning and structured conversational prompts.

#### **Evaluation Metrics Development**
- **Reference-based vs. reference-free**: Research emphasized the importance of matching evaluation protocols with the type of human evaluation being conducted.
- **Multi-aspect evaluation**: New frameworks incorporated multiple dimensions of quality assessment, including accuracy, robustness, and consistency.

## Implications and Future Directions

The ACL 2023 papers collectively suggest that:

1. **Scale matters significantly** for LLM performance, but scaling alone doesn't solve all problems
2. **Current evaluation methods are insufficient** and often inconsistent across different settings
3. **Human evaluation remains crucial** but needs better standardization and protocols
4. **Cross-lingual and multilingual evaluation** requires more attention and resources
5. **Task-specific benchmarks** are needed to properly assess model capabilities in specialized domains

The research community is moving toward more nuanced, multi-dimensional evaluation approaches that better capture the complexities of language understanding and generation in real-world applications.
```

