# HTTP API (`rowcraft serve`)

`rowcraft serve` exposes every saved data source over a small local HTTP API, so you can run SQL from curl, Postman, or a test suite without opening the app.

```sh
rowcraft serve                                   # http://127.0.0.1:7878
rowcraft serve --listen 127.0.0.1:9000
API_TOKEN=change-me rowcraft serve --listen 0.0.0.0:7878 --token-env API_TOKEN
```

The server reads `datasources.toml` on every request. A connection added or edited in the CLI or the desktop app is available immediately, with no restart. Passwords resolve the same way as in `rowcraft query`: from the OS credential store for sources saved in the app, or from `password_env`. SSH tunnels use the saved SSH configuration; their passwords must come from environment variables because the server cannot prompt.

## Endpoints

`{name}` is the data source name, percent-encoded (`my db` becomes `my%20db`).

| Method | Path | Does |
| --- | --- | --- |
| `GET` | `/health` | `{"status": "ok"}` |
| `GET` | `/datasources` | Every saved source, with its endpoints |
| `GET` | `/datasources/{name}` | One source |
| `POST` | `/datasources/{name}/query` | Run SQL (see below) |
| `GET` | `/datasources/{name}/catalog?schema=public` | Schemas, tables, columns, indexes, keys, routines. Repeat `schema` for several; omit it for all |
| `POST` | `/datasources/{name}/test` | Open a fresh connection, then close it |
| `POST` | `/datasources/{name}/cancel` | Cancel the statement running on this source |
| `DELETE` | `/datasources/{name}/session` | Close the open connection; the next query reconnects |

### Running SQL

You can send the SQL in any of three ways:

```sh
# Plain-text body
curl -X POST localhost:7878/datasources/local-pg/query --data-binary 'SELECT now()'

# JSON body
curl -X POST localhost:7878/datasources/local-pg/query \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT * FROM people", "max_rows": 100}'

# Query string (still a POST)
curl -X POST 'localhost:7878/datasources/local-pg/query?sql=SELECT+1'
```

Options can go in the JSON body or the query string; the query string wins:

- `max_rows`: maximum rows returned (default 10000). When more exist, `truncated` is `true`.
- `format`: `json` (the default), `csv`, `tsv`, `ndjson`, `md`, or `table`. Anything other than `json` returns the same text that `rowcraft query -f` writes.

A JSON result looks like this:

```json
{
  "columns": ["id", "email"],
  "rows": [[1, "a@x.io"], [2, null]],
  "row_count": 2,
  "rows_affected": null,
  "truncated": false,
  "elapsed_ms": 3
}
```

Binary values are hex strings. NaN and infinity are returned as strings.

Each data source keeps one open connection, so statements on the same source run one at a time, in the order they arrive, as in a console. Session state such as `SET` and temporary tables carries over between requests until `DELETE /datasources/{name}/session`, or until the source's settings change. If the server closed an idle connection, the API reconnects and retries the statement once.

### Errors

Errors return JSON as `{"error": "…", "kind": "…"}`:

| Status | `kind` | When |
| --- | --- | --- |
| 400 | `usage`, `config` | Missing SQL, bad option, invalid saved settings |
| 401 | `unauthorized` | Token missing or wrong |
| 403 | `forbidden` | Browser request (`Origin` header) or unexpected `Host` |
| 404 | `not_found` | Unknown data source or path |
| 409 | `cancelled` | The statement was cancelled |
| 422 | `statement` | The database rejected the statement |
| 501 | `unsupported` | The dialect cannot connect in this build |
| 502 | `connection` | Cannot connect, authenticate, or open the SSH tunnel |

## Security

Every request runs SQL with your saved credentials, so the server is locked down by default:

- It listens on loopback only, unless you pass `--token-env`. Without a token it refuses any other address.
- With `--token-env VAR`, every request must send `Authorization: Bearer <value of VAR>`.
- Requests that carry an `Origin` header are refused. This blocks browsers, so a web page you visit cannot reach the API. Postman, curl, and test code do not send `Origin`.
- On loopback, the `Host` header must be `localhost` or a loopback IP, which blocks DNS-rebinding attacks.
- SQL is accepted only on `POST`.
- The API does not add a read-only mode of its own. Mark a source **Read-only** in its settings to have the database reject writes.
