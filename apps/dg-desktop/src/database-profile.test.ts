import { expect, it } from "vitest";
import { databaseProfile } from "./database-profile";
import { sqlMessages, suggestedSql } from "./ai";
import { supportsConsoleSchema } from "./console-schema";
it("uses native query starters and namespaces for each database family", () => {
  expect(databaseProfile("mongo")).toMatchObject({starter:"show collections\n",objects:"Collections",namespace:"Database",documentView:true});
  expect(databaseProfile("redis").starter).toBe("SCAN 0 COUNT 100\n");
  expect(databaseProfile("neo4j").language).toBe("Cypher");
  expect(databaseProfile("graphql").language).toBe("GraphQL");
  expect(databaseProfile("mysql").starter).toBe("select 1;\n");
  expect(supportsConsoleSchema("mongo")).toBe(true);
  expect(supportsConsoleSchema("redis")).toBe(true);
});
it("instructs AI to produce native commands and extracts native code fences", () => {
  for (const dialect of ["mongo","redis","neo4j","graphql","cassandra"]) {
    const profile = databaseProfile(dialect);
    const messages = sqlMessages("generate","List objects","",dialect,"",null,false);
    expect(messages[0]?.content).toContain(`Use ${profile.language} syntax`);
    expect(suggestedSql(`\u0060\u0060\u0060${profile.fence}\n${profile.starter}\u0060\u0060\u0060`)).toBe(profile.starter.trim());
  }
});
