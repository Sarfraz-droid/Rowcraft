import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { sql } from "@codemirror/lang-sql";
import type { Extension } from "@codemirror/state";
import type { CompletionContext } from "@codemirror/autocomplete";
import type { Catalog } from "./api";
import { databaseProfile } from "./database-profile";
import { editorSqlConfig } from "./editor-completions";

export function editorLanguage(dialect: string, catalog: Catalog | null, schema?: string): Extension {
  const profile = databaseProfile(dialect);
  if (["SQL", "CQL", "SQL++", "PartiQL"].includes(profile.language)) return sql(editorSqlConfig(dialect, catalog, schema));
  const keywords = dialect === "redis" ? "GET SET SCAN MATCH COUNT TYPE TTL HGETALL HSCAN LRANGE SMEMBERS SSCAN ZRANGE ZSCAN PING INFO SELECT DBSIZE EXISTS STRLEN MGET" : dialect === "neo4j" ? "MATCH OPTIONAL WHERE RETURN WITH LIMIT ORDER BY ASC DESC UNWIND AS CREATE MERGE SET DELETE DETACH REMOVE CALL YIELD DISTINCT AND OR NOT NULL TRUE FALSE" : dialect === "graphql" ? "query mutation subscription fragment on true false null" : "db show collections databases use find findOne aggregate countDocuments getCollection getCollectionNames runCommand sort limit skip ObjectId ISODate true false null";
  const words = new Set(keywords.split(" "));
  const parser: StreamParser<null> = { startState: () => null, token(stream) {
    if (stream.eatSpace()) return null;
    if (stream.match(/\/\/.*|#.*|--.*/)) return "comment";
    if (stream.match(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`]|``)*`/)) return "string";
    if (stream.match(/-?\d+(?:\.\d+)?/)) return "number";
    const word = stream.match(/[$A-Za-z_][\w$]*/);
    if (word && word !== true) return words.has(word[0]) || words.has(word[0].toUpperCase()) ? "keyword" : null;
    stream.next(); return null;
  } };
  const language = StreamLanguage.define(parser);
  const complete = (context: CompletionContext) => {
    const word = context.matchBefore(/[\w$]*/);
    if (!word || (!context.explicit && word.from === word.to)) return null;
    const objects = (catalog?.schemas ?? []).filter(item => !schema || item.name === schema).flatMap(item => item.tables);
    return { from: word.from, options: [
      ...keywords.split(" ").map(label => ({label, type:"keyword", detail:profile.language})),
      ...objects.map(table => ({label:table.name,type:"type",detail:profile.object})),
      ...objects.flatMap(table => table.columns.map(column => ({label:column.name,type:"property",detail:column.native_type}))),
    ] };
  };
  return [language, language.data.of({ autocomplete: complete })];
}
