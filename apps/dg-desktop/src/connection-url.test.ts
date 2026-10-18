import { expect, it } from "vitest";
import { parseConnectionUrl } from "./connection-url";

it("extracts MongoDB fields and authentication options without changing the selected driver", () => {
  expect(parseConnectionUrl("mongodb://reader@db.example.test:27018/community?authMechanism=DEFAULT&authSource=community", "existing=keep"))
    .toEqual({ host: "db.example.test", port: "27018", database: "community", user: "reader", auth: "user-password", properties: "existing=keep\nauthMechanism=DEFAULT\nauthSource=community", url: "mongodb://reader@db.example.test:27018/community?authMechanism=DEFAULT&authSource=community" });
});

it("supports SRV URLs, bracketed IPv6 and replica-set host lists", () => {
  expect(parseConnectionUrl("mongodb+srv://reader@cluster.example.test/app?tls=true")).toMatchObject({ host: "cluster.example.test", database: "app", user: "reader", ssl_mode: "verify-full" });
  expect(parseConnectionUrl("postgresql://[::1]:5544/app")).toMatchObject({ host: "::1", port: "5544", database: "app" });
  expect(parseConnectionUrl("mongodb://one.example:27017,two.example:27018/app")).toMatchObject({ host: "one.example:27017,two.example:27018", database: "app" });
});

it("decodes fields and removes embedded passwords from URLs and driver properties", () => {
  const parsed = parseConnectionUrl("jdbc:postgresql://demo%40team:only%3Atest@db.example/app%20data?application_name=test&password=other%20test");
  expect(parsed).toMatchObject({ user: "demo@team", password: "other test", password_env: "", clear_password: false, database: "app data", properties: "application_name=test", url: "jdbc:postgresql://demo%40team@db.example/app%20data?application_name=test" });
});

it("leaves omitted fields unchanged and accepts a user query parameter", () => {
  const parsed = parseConnectionUrl("postgresql://db.example");
  expect(parsed).toEqual({ host: "db.example", url: "postgresql://db.example" });
  expect(parseConnectionUrl("postgresql://db.example/app?user=reader&sslmode=verify-full")).toMatchObject({ user: "reader", auth: "user-password", ssl_mode: "verify-full" });
});

it("ignores incomplete and malformed URLs instead of clearing connection fields", () => {
  for (const value of ["", "mongodb://", "mongodb://db.example:", "mongodb://db.example:99999/app", "mongodb://[::1/app", "postgresql://reader%GG@db.example/app", "jdbc:h2:mem:example"]) {
    expect(parseConnectionUrl(value)).toBeNull();
  }
});
