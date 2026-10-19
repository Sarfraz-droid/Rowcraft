import type { Catalog } from "../api";
import { databaseProfile } from "../database-profile";
export function QueryHelp({ dialect, catalog, schema, onInsert }: {dialect: string; catalog: Catalog | null; schema: string; onInsert(query: string): void}) {
  const profile = databaseProfile(dialect);
  const object = catalog?.schemas.find(item => item.name === schema)?.tables[0]?.name;
  const examples = [profile.starter.trim()];
  if (object && dialect === "mongo") examples.push(`db.getCollection(${JSON.stringify(object)}).find({}).limit(10)`, `db.getCollection(${JSON.stringify(object)}).countDocuments({})`);
  if (dialect === "redis") examples.push("INFO keyspace", "DBSIZE");
  if (dialect === "neo4j") examples.push("MATCH (a)-[r]->(b) RETURN a, r, b LIMIT 25;");
  return <details className="console-query-help"><summary>{profile.language} · Examples & help</summary><div><p>{dialect === "redis" ? "Run native Redis commands. SCAN returns a cursor; repeat with that cursor until it returns 0. COUNT is a hint." : dialect === "mongo" ? "Use MongoDB shell commands and pipelines. This console does not run arbitrary JavaScript. Choose the database above." : dialect === "graphql" ? "Use a GraphQL document, or a JSON request containing query, variables, and operationName. Queries requiring arguments must be written explicitly." : `Write ${profile.language} queries for this connection. Select a statement to run it on its own.`}</p>{examples.map(query => <div key={query}><pre>{query}</pre><button type="button" className="btn ghost" onClick={() => onInsert(query)}>Insert example</button></div>)}</div></details>;
}
