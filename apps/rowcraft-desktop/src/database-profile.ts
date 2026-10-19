export interface DatabaseProfile {
  language: string;
  fence: string;
  console: string;
  namespace: string;
  objects: string;
  object: string;
  fields: string;
  records: string;
  starter: string;
  documentView: boolean;
  relationships: boolean;
}
const sql: DatabaseProfile = { language: "SQL", fence: "sql", console: "SQL console", namespace: "Schema", objects: "Tables", object: "table", fields: "Columns", records: "rows", starter: "select 1;\n", documentView: false, relationships: true };
/** One vocabulary and query-language contract for console, explorer, results, and AI. */
export function databaseProfile(dialect = ""): DatabaseProfile {
  switch (dialect) {
    case "mongo": return { ...sql, language: "MongoDB shell", fence: "javascript", console: "MongoDB console", namespace: "Database", objects: "Collections", object: "collection", fields: "Fields", records: "documents", starter: "show collections\n", documentView: true, relationships: false };
    case "redis": return { ...sql, language: "Redis", fence: "redis", console: "Redis console", namespace: "Database", objects: "Key groups", object: "key group", fields: "Key types", records: "entries", starter: "SCAN 0 COUNT 100\n", documentView: false, relationships: false };
    case "cassandra": return { ...sql, language: "CQL", fence: "cql", console: "CQL console", namespace: "Keyspace", starter: "SELECT keyspace_name FROM system_schema.keyspaces;\n", relationships: false };
    case "dynamo": return { ...sql, language: "PartiQL", fence: "sql", console: "PartiQL console", namespace: "Region", fields: "Attributes", records: "items", starter: "show tables\n", documentView: true, relationships: false };
    case "couchbase": return { ...sql, language: "SQL++", console: "SQL++ console", namespace: "Bucket", objects: "Collections", object: "collection", fields: "Fields", records: "documents", starter: "SELECT RAW name FROM system:keyspaces LIMIT 25;\n", documentView: true, relationships: false };
    case "neo4j": return { ...sql, language: "Cypher", fence: "cypher", console: "Cypher console", namespace: "Database", objects: "Node labels", object: "node label", fields: "Properties", records: "records", starter: "MATCH (n) RETURN n LIMIT 25;\n", documentView: true, relationships: false };
    case "graphql": return { ...sql, language: "GraphQL", fence: "graphql", console: "GraphQL console", namespace: "API", objects: "Queries", object: "query", fields: "Fields", records: "records", starter: "query {\n  __typename\n}\n", documentView: true, relationships: false };
    default: return sql;
  }
}
