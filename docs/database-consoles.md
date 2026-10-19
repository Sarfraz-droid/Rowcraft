# Database consoles and support boundaries

The selected source determines the console language, namespace selector, explorer vocabulary,
query examples, AI instructions, preview statement, and initial result presentation.

| Source family | Console | Objects / namespace | Result presentation |
| --- | --- | --- | --- |
| PostgreSQL, MySQL, SQLite, SQL Server, Oracle, warehouses and JDBC SQL sources | SQL with available dialect completion | Tables and views / schemas | Table and chart |
| MongoDB / DocumentDB | MongoDB shell statements | Collections / databases | JSON documents, table and chart |
| Redis | Redis commands | Key prefix groups / numeric databases | Command reply table; SCAN cursor explained |
| Cassandra | CQL | Tables / keyspaces | Table and chart |
| DynamoDB | PartiQL and supported discovery commands | Tables / regions | JSON items, table and chart |
| Couchbase | SQL++ | Collections / bucket.scope | JSON documents, table and chart |
| Neo4j | Cypher over HTTP Query API v2 | Sampled node labels / connected graph | JSON records, table and chart |
| GraphQL APIs | GraphQL over HTTP | Root queries / query type | JSON records, table and chart |

MongoDB previews use `db.getCollection("name").find({}).limit(2000)`, with JSON-escaped names.
Introspection restores its original database even after discovery fails. Nested BSON is encoded
as canonical Extended JSON, preserving tagged BSON types rather than producing invalid JSON text.
The shell supports the commands implemented by the driver; it is not an arbitrary JavaScript runtime.

Redis previews use SCAN with an escaped key-prefix pattern. COUNT is a hint, not a record limit.
Continue with the returned cursor until it reaches zero. Use TYPE before choosing GET, HGETALL,
LRANGE, or another type-specific command. Cluster mode cannot switch away from database zero.

Couchbase preview identifiers quote bucket, scope and collection individually. DynamoDB previews
never treat the region as a SQL schema. Native MongoDB, Redis, Cypher and GraphQL documents bypass
SQL limit rewriting. JSON results paginate 25 records at a time; this is presentation pagination,
not a replacement for server-side query bounds.

## Connection lifetime across tabs

Each connected data source retains its backend session and latest discovered catalog for the app's
lifetime. Switching tabs or workspaces selects the existing session without opening another
connection, rediscovering metadata, or rerunning existing results. Consoles for the same source
share that source's session, including temporary objects and transaction state. Closing a tab does
not disconnect its source. Connection indicators remain enabled for all retained sources.

Use **Refresh** to rediscover metadata and **Reconnect** to replace only the selected source's
session. Editing or removing a source invalidates its retained connection; other sources remain
available. Connections are not persisted across app restarts, and a server can still close an idle
session independently of tab navigation.

## MongoDB object map references

The MongoDB object map supports inferred and manual reference links. **Infer references** is on by
default. A field such as `userId`, `user_id`, or `user_ids` can suggest a link to `users._id` when
sampled metadata reports an eligible identifier/array type and exactly one matching collection in
the same database. Simple singular/plural normalization includes `communities` / `community`.
Ambiguous names, absent target fields, and cross-database guesses are skipped. No document values
are compared; dashed links are unverified hints and are not MongoDB constraints. This is not DBRef
or embedded-document discovery, and it cannot guarantee every relationship is found.

Expand **Manage references**, choose a source collection/field and target collection/field, then
select **Add reference**. Target defaults to `_id` when available. Manual links use dotted lines and
override inferred hints for the same source field. Remove a manual link to restore any applicable
inferred hint. Turn off inference to view only manual references. Search includes related neighbors;
unlinked collections can be included with **All collections**.

Manual links are saved in this desktop/browser profile's local storage, scoped by source name and
host. They survive reopening the map but do not sync to other machines; renaming a source or changing
its host uses a new scope. Missing fields do not delete saved links: they appear as unavailable in
the manager until metadata returns or the link is removed. Storage failures are shown without
claiming that the link was saved. No MongoDB documents or constraints are changed.

## Neo4j and GraphQL setup

Add **Neo4j (HTTP Query API)** or **GraphQL (HTTP API)** in the data source dialog. Enter a full endpoint
URL or a host and port. Default paths are `/db/<database>/query/v2` (port 7474) and `/graphql` (port 4000).
Basic authentication uses the user and password fields. For a bearer token, leave user blank and enter
the token as the password. Explicit URLs must use HTTP(S), without credentials or query parameters.
Remote host defaults use HTTPS; loopback host defaults use HTTP. These connectors do not implement
custom CA/client certificate settings, Bolt, Gremlin, GraphQL subscriptions, or custom auth headers.

Cypher accepts a statement or a JSON envelope with `statement` and `parameters`. GraphQL accepts a
query document or an envelope with `query`, `variables`, and optional `operationName`:

```json
{
  "query": "query User($id: ID!) { user(id: $id) { id name } }",
  "variables": { "id": "001" },
  "operationName": "User"
}
```

GraphQL introspection must be enabled to browse the schema. Previews select scalar fields and skip
fields requiring arguments. Root queries requiring arguments must be written explicitly in the
console. Neo4j label/property discovery samples up to 1,000 nodes; absent labels are not proof they
are absent from the graph. Graph query results have no interactive node-edge renderer yet.

HTTP responses are bounded to 8 MB and 45 seconds. Server errors are checked even on HTTP success.
Read-only mode conservatively screens query text and can reject otherwise valid queries. Use
server-side read credentials to enforce permissions. Query responses are normalized into the common
result model; raw GraphQL response envelopes/extensions are not retained as a separate view.

Protocol references: [Neo4j Query API](https://neo4j.com/docs/query-api/current/query/) and
[GraphQL over HTTP](https://graphql.org/learn/serving-over-http/).

## Extending database coverage

The built-in catalog includes native connections, protocol-compatible variants and JDBC entries.
A catalog entry does not imply every vendor/version has been tested. JDBC requires Java and the
vendor's JARs. A custom driver can reuse an existing native protocol or provide a JDBC driver class,
JARs and URL template. A database with a different protocol requires a connector implementation;
there is no universal driver that supports every database in the world.

AI generates the selected native language. Its separate automatic read-only SQL execution tool
remains restricted to its validated SQL backends; adding a console does not extend that tool's
execution permissions.
