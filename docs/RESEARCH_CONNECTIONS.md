# Research connections

CausalMed's research bridge makes an aggregate expression-QC snapshot available to Gemini, OpenAI, local model servers, and MCP clients. It runs from the command line alongside the Tauri application. Model responses are exploratory research notes. Use CausalMed's existing evidence review and evaluation for checked results.

日本語 — H5ADの発現QC集計をGemini、OpenAI、ローカルモデルサーバー、MCPクライアントから利用できます。モデルの回答は検証前の研究メモです。日本語で回答させるにはサブコマンドの前に `--lang ja` を指定してください。

简体中文 — 可通过 Gemini、OpenAI、本地模型服务和 MCP 客户端使用 H5AD 表达量质控摘要。模型回答是待验证的研究笔记。在子命令前指定 `--lang zh-CN` 可请求简体中文回答。

## Reuse saved desktop QC

To use the exact thresholds and decisions of a saved desktop run, select that QC history entry and choose **Export QC results (ZIP)**. Generate a research snapshot from the exported archive with Python's standard library.

```sh
PYTHONPATH=python python3 -m causalmed_worker.research snapshot-qc /absolute/saved-qc.zip \
  --output /absolute/qc-summary.json
```

This route reads only the archive manifest. It records the QC analysis ID and the SHA-256 of the exported manifest alongside the original source hash. Counts for duplicate IDs are unknown because the ZIP manifest does not include them. The QC archive itself contains per-cell data. Share the derived JSON for model review.

日本語 — QC履歴から「QC結果をエクスポート（ZIP）」を選び、上記の `snapshot-qc` で研究用JSONを作ります。原本H5ADの再解析は不要です。

简体中文 — 从质控历史中导出 QC 结果 ZIP，再用 `snapshot-qc` 生成研究用 JSON。无需重新分析原始 H5AD。

## Prepare a QC snapshot

Run from the repository root with a Python environment that has the worker dependencies. Set the donor column if one is available. The example uses the default `unconfirmed` origin. Use `--origin real` or `--origin synthetic` when you have established the source type.

```sh
PYTHONPATH=python python3 -m causalmed_worker.research snapshot /absolute/source.h5ad \
  --donor-column replicate --output /absolute/qc-summary.json
```

The snapshot includes a source SHA-256, QC status, aggregate counts and numeric thresholds. It omits expression matrices, per-cell values, cell IDs, donor labels, annotation labels, filenames and paths. Review the summary and the question before sending them to any model. The original H5AD is opened read-only.

## Ask a model

Set the API key in your shell environment for the chosen provider: `OPENAI_API_KEY` or `GEMINI_API_KEY`. Supply a model ID available to your account. OpenAI uses the Responses API with response storage disabled. Gemini uses the native Gemini API. The local route accepts an OpenAI-compatible Chat Completions endpoint on loopback, such as Ollama's `/v1` endpoint, and optionally reads `CAUSALMED_LOCAL_API_KEY`.

```sh
PYTHONPATH=python python3 -m causalmed_worker.research --lang ja ask \
  --snapshot /absolute/qc-summary.json --provider gemini --model YOUR_MODEL_ID \
  --question 'QCの制約と、次に確認すべき点を挙げてください。'
```

For OpenAI substitute `--provider openai --model YOUR_MODEL_ID`. For local inference substitute `--provider local --model YOUR_LOCAL_MODEL --base-url http://127.0.0.1:11434/v1`. The question and summary are sent to the selected endpoint. The result includes the provider, requested model, source hash and an explicit research-note designation. [OpenAI Responses API](https://developers.openai.com/api/reference/resources/responses/methods/create) · [Gemini API](https://ai.google.dev/api/generate-content)

## Codex and Google Antigravity App

Use absolute paths in your client configuration. The stdio server exposes one read-only tool, `get_qc_summary`. It uses the snapshot selected at launch and cannot accept other file paths. Restart the MCP connection to load a different snapshot. A cloud-backed MCP client may include the returned summary in its model context.

Codex configuration in `config.toml`:

```toml
[mcp_servers.causalmed]
command = "/absolute/path/to/python3"
args = ["/absolute/CausalMed/python/causalmed_worker/research.py", "--lang", "ja", "mcp", "--snapshot", "/absolute/qc-summary.json"]
```

[Codex MCP configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)

In the standalone Google Antigravity App, open **Customizations → Installed → MCP Servers** and use **Copy Config File Path**. Merge the following entry into the existing configuration and restart the connection.

```json
{
  "mcpServers": {
    "causalmed": {
      "command": "/absolute/path/to/python3",
      "args": ["/absolute/CausalMed/python/causalmed_worker/research.py", "--lang", "ja", "mcp", "--snapshot", "/absolute/qc-summary.json"]
    }
  }
}
```

[Google Antigravity App MCP configuration](https://antigravity.google/docs/mcp)

Example prompt: “Read the CausalMed QC summary. Explain what the counts support, flag incomplete checks, and suggest follow-up analyses as hypotheses.”
