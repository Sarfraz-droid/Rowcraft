# MCP server (`rowcraft mcp`)

`rowcraft mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio, so an AI client such as Claude Code, Codex, Cursor or Claude Desktop can look at and query the data sources you saved with the CLI or the desktop app. It is the same engine, drivers, SSH tunnels and `datasources.toml` as the rest of Rowcraft. Passwords are read from the environment variable named in each data source, and are never sent to the client.

## Register it with your clients

```sh
rowcraft mcp install                    # every client found on PATH (claude, codex)
rowcraft mcp install --client claude    # only Claude Code
rowcraft mcp install --dry-run          # print the commands without running them
rowcraft mcp config                     # JSON for any other client
```

`install` runs `claude mcp add --scope user rowcraft -- <rowcraft> mcp` and `codex mcp add rowcraft -- <rowcraft> mcp`, using the path of the binary you ran. Run it from an installed copy, not from `target/`, or the clients break when the build is cleaned. `--scope local|project` changes where Claude Code stores the entry.

For Cursor, Claude Desktop and others, paste the output of `rowcraft mcp config` into their MCP settings (`mcpServers`).

Passwords for data sources that use `--password-env` must be visible to the client's process: start the client from a shell where the variable is set.

## Tools

| Tool | What it does |
| --- | --- |
| `list_datasources` | Saved data sources: name, dialect, host, database, user, read-only flag. |
| `get_catalog` | Schemas, tables, columns, indexes, keys and routines. Optional `schemas` filter. |
| `run_query` | Runs one statement; returns columns, rows, `truncated` and timing. `max_rows` defaults to 200 (limit 5,000). |
| `test_connection` | Connects and reports success or the error. |

Failures such as bad SQL or an unknown data source come back as tool errors with the message, so the assistant can correct itself.

## Read-only by default

Without `--allow-write`, `run_query` refuses anything but a single `SELECT`, `WITH`, `SHOW`, `DESCRIBE`, `EXPLAIN`, `VALUES` or `TABLE` statement, and refuses statements that contain write keywords (`INSERT`, `UPDATE`, `DELETE`, `DROP`, `SET`, `INTO`, …). Non-SQL databases such as Redis and MongoDB need `--allow-write` for any command.

This check protects against an assistant's mistake. It is not a security boundary: a function with side effects or a vendor extension can get past it. To enforce read-only access, mark the data source `--read-only` or connect with a database user that can only read.

To allow changes, register with the flag:

```sh
rowcraft mcp install --allow-write
```

## Try it by hand

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | rowcraft mcp
```

The transport is newline-delimited JSON-RPC 2.0; only protocol messages go to stdout and logs go to stderr. Replies may arrive out of order because calls run concurrently.
