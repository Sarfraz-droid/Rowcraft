//! MongoDB dialect (tier A, see SPEC.md §5), on the official `mongodb` driver with rustls.
//!
//! The console takes shell-style statements ([`shell`]); every one is turned into a database
//! command and sent with `run_command`, so the driver API surface stays small and every server
//! feature the command set offers is reachable (a bare `{...}` document runs as a command).
//! Also serves Amazon DocumentDB. Pure Rust, so there is no JavaScript engine: shell syntax
//! covers calls and relaxed-JSON arguments only (see [`shell`]).

mod shell;

use async_trait::async_trait;
use dg_config::{DataSourceConfig, SslMode};
use dg_dialect::{
    Capabilities, Connection, Dialect, DialectFamily, DialectId, Error, IntrospectScope, Result,
    ResultSet, Value, split_statements,
};
use dg_model::{Catalog, Column, Index, Schema, Table};
use mongodb::{
    Client,
    bson::{Bson, Document, doc, oid::ObjectId},
    options::{ClientOptions, Credential, Tls, TlsOptions},
};
use shell::{Statement, parse_statement};

/// Collections sampled for fields in the explorer, and documents sampled per collection.
const MAX_COLLECTIONS_SAMPLED: usize = 300;
const SAMPLE_DOCS: i32 = 20;

#[derive(Debug, Default, Clone, Copy)]
pub struct MongoDialect;

#[async_trait]
impl Dialect for MongoDialect {
    fn id(&self) -> DialectId {
        DialectId("mongo")
    }

    fn display_name(&self) -> &'static str {
        "MongoDB"
    }

    fn family(&self) -> DialectFamily {
        DialectFamily::Document
    }

    fn default_port(&self) -> Option<u16> {
        Some(27017)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn connects(&self) -> bool {
        true
    }

    async fn connect(&self, cfg: &DataSourceConfig) -> Result<Box<dyn Connection>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let options = client_options(cfg).await?;
        let client = Client::with_options(options).map_err(|e| Error::Connect(e.to_string()))?;
        let database =
            cfg.database.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| "test".into());
        // Server selection is lazy; a ping surfaces bad hosts and credentials now.
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| Error::Connect(e.to_string()))?;
        Ok(Box::new(MongoConnection { client, database, read_only: cfg.read_only }))
    }

    async fn introspect(
        &self,
        conn: &mut dyn Connection,
        scope: &IntrospectScope,
    ) -> Result<Catalog> {
        let name = conn.query("db.getName()").await?;
        let original = name
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(|value| match value {
                Value::Text(name) => Some(name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "test".into());
        let result = async {
            let dbs = conn.query("show dbs").await?;
            let mut names: Vec<String> = dbs
                .rows
                .iter()
                .filter_map(|r| match r.first() {
                    Some(Value::Text(s)) => Some(s.clone()),
                    _ => None,
                })
                .filter(|n| !matches!(n.as_str(), "admin" | "local" | "config"))
                .filter(|n| scope.schemas.as_ref().is_none_or(|only| only.contains(n)))
                .collect();
            names.sort();
            let mut schemas = Vec::new();
            let mut sampled = 0;
            for db in names {
                conn.query(&format!("use {db}")).await?;
                let colls = conn.query("show collections").await?;
                let mut tables = Vec::new();
                for row in &colls.rows {
                    let Some(Value::Text(name)) = row.first() else { continue };
                    if name.starts_with("system.") {
                        continue;
                    }
                    let mut table = Table { name: name.clone(), ..Table::default() };
                    if sampled < MAX_COLLECTIONS_SAMPLED {
                        sampled += 1;
                        let call = |m: &str| format!("db.getCollection({}).{m}", quote(name));
                        if let Ok(sample) =
                            conn.query(&call(&format!("find({{}}).limit({SAMPLE_DOCS})"))).await
                        {
                            table.columns = infer_columns(&sample);
                        }
                        if let Ok(ix) = conn.query(&call("getIndexes()")).await {
                            table.indexes = indexes(&ix);
                        }
                    }
                    tables.push(table);
                }
                schemas.push(Schema { name: db, tables, ..Schema::default() });
            }
            Ok(Catalog {
                database: Some(original.clone()),
                default_schema: Some(original.clone()),
                schemas,
            })
        }
        .await;
        let restored = conn.query(&format!("use {original}")).await;
        match (result, restored) {
            (Ok(catalog), Ok(_)) => Ok(catalog),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

async fn client_options(cfg: &DataSourceConfig) -> Result<ClientOptions> {
    let host = cfg.host.as_deref().unwrap_or("localhost").trim();
    // A full connection string in Host wins; it carries its own options and credentials.
    let uri = if host.starts_with("mongodb://") || host.starts_with("mongodb+srv://") {
        host.to_owned()
    } else {
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        format!("mongodb://{host}:{}", cfg.port.unwrap_or(27017))
    };
    let mut options =
        ClientOptions::parse(&uri).await.map_err(|e| Error::Connect(e.to_string()))?;
    options.app_name = Some("Rowcraft".into());
    options.server_selection_timeout = Some(std::time::Duration::from_secs(10));
    if let Some(user) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        // Users usually live in `admin`; the data source's database is the default to work in.
        let source = cfg.properties.get("authSource").cloned().unwrap_or_else(|| "admin".into());
        options.credential = Some(
            Credential::builder()
                .username(user.to_owned())
                .password(cfg.password.clone())
                .source(source)
                .build(),
        );
    }
    let tls_options = || {
        TlsOptions::builder().ca_file_path(cfg.ssl_files.ca_file.clone().map(Into::into)).build()
    };
    match cfg.ssl_mode {
        SslMode::Disable => options.tls = Some(Tls::Disabled),
        // Follow MongoDB's defaults for both host fields and full URIs: ordinary
        // mongodb:// connections use plaintext unless TLS was explicitly enabled,
        // while mongodb+srv:// enables TLS during URI parsing.
        SslMode::Auto => {}
        SslMode::VerifyFull | SslMode::VerifyCa => options.tls = Some(Tls::Enabled(tls_options())),
        SslMode::Require => {
            let mut t = tls_options();
            t.allow_invalid_certificates = Some(true);
            options.tls = Some(Tls::Enabled(t));
        }
    }
    Ok(options)
}

#[derive(Debug)]
struct MongoConnection {
    client: Client,
    database: String,
    read_only: bool,
}

#[async_trait]
impl Connection for MongoConnection {
    async fn query_limited(&mut self, statement: &str, max_rows: usize) -> Result<ResultSet> {
        let mut last = ResultSet::default();
        // Newlines separate statements too, as in a shell session.
        let mut pieces: Vec<&str> = Vec::new();
        for chunk in split_statements(statement) {
            let one_line = !chunk.contains('\n') || parse_statement(chunk).is_ok();
            if one_line {
                pieces.push(chunk);
            } else {
                pieces.extend(chunk.lines().map(str::trim).filter(|l| !l.is_empty()));
            }
        }
        for piece in pieces {
            let stmt = parse_statement(piece).map_err(Error::Query)?;
            last = self.run(stmt, max_rows).await?;
        }
        Ok(last)
    }
}

impl MongoConnection {
    async fn command(&self, db: &str, command: Document) -> Result<Document> {
        self.client.database(db).run_command(command).await.map_err(|e| Error::Query(e.to_string()))
    }

    /// Reads a cursor reply (`firstBatch`, then `getMore`) until `max_rows` documents are held.
    async fn cursor_rows(&self, reply: Document, max_rows: usize) -> Result<ResultSet> {
        let mut docs: Vec<Document> = Vec::new();
        let mut truncated = false;
        let mut cursor =
            reply.get_document("cursor").map_err(|e| Error::Query(e.to_string()))?.clone();
        loop {
            let batch = cursor
                .get_array("firstBatch")
                .or_else(|_| cursor.get_array("nextBatch"))
                .cloned()
                .unwrap_or_default();
            for item in batch {
                if docs.len() >= max_rows {
                    truncated = true;
                    break;
                }
                if let Bson::Document(d) = item {
                    docs.push(d);
                }
            }
            let id = cursor.get_i64("id").unwrap_or(0);
            let ns = cursor.get_str("ns").unwrap_or_default().to_owned();
            let (db, coll) = ns.split_once('.').unwrap_or(("", ""));
            if id == 0 {
                break;
            }
            if truncated || docs.len() >= max_rows {
                // Free the server-side cursor; failure here is harmless (it times out anyway).
                let _ = self.command(db, doc! { "killCursors": coll, "cursors": [id] }).await;
                truncated = truncated || docs.len() >= max_rows;
                break;
            }
            let next = self.command(db, doc! { "getMore": id, "collection": coll }).await?;
            cursor = next.get_document("cursor").map_err(|e| Error::Query(e.to_string()))?.clone();
        }
        let mut rs = documents_to_result(&docs);
        rs.truncated = truncated;
        Ok(rs)
    }

    async fn run(&mut self, stmt: Statement, max_rows: usize) -> Result<ResultSet> {
        match stmt {
            Statement::Use(db) => {
                self.database = db;
                Ok(ResultSet {
                    columns: vec!["database".into()],
                    rows: vec![vec![Value::Text(self.database.clone())]],
                    ..ResultSet::default()
                })
            }
            Statement::ShowDatabases => {
                let reply =
                    self.command("admin", doc! { "listDatabases": 1, "nameOnly": true }).await?;
                let rows = reply
                    .get_array("databases")
                    .map(|a| {
                        a.iter()
                            .filter_map(|d| {
                                Some(vec![Value::Text(
                                    d.as_document()?.get_str("name").ok()?.to_owned(),
                                )])
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(ResultSet { columns: vec!["name".into()], rows, ..ResultSet::default() })
            }
            Statement::ShowCollections => self.list_collections(max_rows).await,
            Statement::Database { method, args } => match method.as_str() {
                "getCollectionNames" => self.list_collections(max_rows).await,
                "runCommand" => {
                    let command = first_document(&args, "runCommand")?;
                    self.guard(command.keys().next().map_or("", String::as_str))?;
                    let reply = self.command(&self.database.clone(), command).await?;
                    if reply.contains_key("cursor") {
                        self.cursor_rows(reply, max_rows).await
                    } else {
                        Ok(documents_to_result(&[reply]))
                    }
                }
                "getName" => Ok(single("database", Value::Text(self.database.clone()))),
                other => Err(Error::Query(format!("db.{other}() is not supported"))),
            },
            Statement::Collection { name, method, args, chain } => {
                self.collection_call(&name, &method, &args, &chain, max_rows).await
            }
        }
    }

    async fn list_collections(&self, max_rows: usize) -> Result<ResultSet> {
        let reply = self
            .command(&self.database, doc! { "listCollections": 1, "nameOnly": true, "cursor": {} })
            .await?;
        let all = self.cursor_rows(reply, max_rows).await?;
        let at = all.columns.iter().position(|c| c == "name").unwrap_or(0);
        Ok(ResultSet {
            columns: vec!["name".into()],
            rows: all
                .rows
                .into_iter()
                .map(|r| vec![r.get(at).cloned().unwrap_or(Value::Null)])
                .collect(),
            truncated: all.truncated,
            ..ResultSet::default()
        })
    }

    /// Read-only data sources refuse commands that write.
    fn guard(&self, command: &str) -> Result<()> {
        const WRITES: &[&str] = &[
            "insert",
            "update",
            "delete",
            "drop",
            "dropdatabase",
            "createindexes",
            "dropindexes",
            "create",
            "renamecollection",
            "findandmodify",
            "collmod",
            "createuser",
            "dropuser",
            "shutdown",
        ];
        if self.read_only && WRITES.contains(&command.to_ascii_lowercase().as_str()) {
            return Err(Error::Query(format!(
                "`{command}` is not allowed on a read-only data source"
            )));
        }
        Ok(())
    }

    async fn collection_call(
        &self,
        coll: &str,
        method: &str,
        args: &[Bson],
        chain: &[(String, Vec<Bson>)],
        max_rows: usize,
    ) -> Result<ResultSet> {
        let db = self.database.clone();
        let arg = |i: usize| args.get(i).cloned();
        let document = |i: usize| -> Result<Document> {
            match args.get(i) {
                None => Ok(Document::new()),
                Some(Bson::Document(d)) => Ok(d.clone()),
                Some(_) => {
                    Err(Error::Query(format!("{method}(): argument {} must be a document", i + 1)))
                }
            }
        };
        if !matches!(method, "find" | "findOne") && !chain.is_empty() {
            return Err(Error::Query(format!(".{}() can only follow find()", chain[0].0)));
        }
        let write = |reply: Document, keys: &[&str]| -> ResultSet {
            let n = keys.iter().filter_map(|k| reply.get(k)).filter_map(as_u64).next();
            let mut rs = ResultSet { rows_affected: n, ..ResultSet::default() };
            if let Ok(errors) = reply.get_array("writeErrors") {
                rs.columns = vec!["writeErrors".into()];
                rs.rows = errors.iter().map(|e| vec![Value::Text(to_json(e))]).collect();
            }
            rs
        };
        match method {
            "find" | "findOne" => {
                let mut cmd = doc! { "find": coll, "filter": document(0)? };
                if let Some(p) = arg(1) {
                    cmd.insert("projection", p);
                }
                for (name, a) in chain {
                    match (name.as_str(), a.first()) {
                        ("sort", Some(v)) => cmd.insert("sort", v.clone()),
                        ("projection" | "project", Some(v)) => cmd.insert("projection", v.clone()),
                        ("limit", Some(v)) => cmd.insert("limit", as_i64(v).unwrap_or(0)),
                        ("skip", Some(v)) => cmd.insert("skip", as_i64(v).unwrap_or(0)),
                        ("batchSize" | "toArray" | "pretty" | "maxTimeMS", _) => None,
                        (other, _) => {
                            return Err(Error::Query(format!(
                                "cursor method .{other}() is not supported"
                            )));
                        }
                    };
                }
                if method == "findOne" {
                    cmd.insert("limit", 1_i64);
                    cmd.insert("singleBatch", true);
                }
                // Fetch one more than the caller keeps so `truncated` is exact.
                if max_rows != usize::MAX {
                    cmd.insert(
                        "batchSize",
                        i64::try_from(max_rows.saturating_add(1)).unwrap_or(i64::MAX).min(10_000),
                    );
                }
                let reply = self.command(&db, cmd).await?;
                self.cursor_rows(reply, max_rows).await
            }
            "aggregate" => {
                let pipeline = match arg(0) {
                    Some(Bson::Array(a)) => a,
                    _ => return Err(Error::Query("aggregate() needs a pipeline array".into())),
                };
                if self.read_only
                    && pipeline.iter().any(|s| {
                        s.as_document()
                            .is_some_and(|d| d.contains_key("$out") || d.contains_key("$merge"))
                    })
                {
                    return Err(Error::Query(
                        "$out and $merge are not allowed on a read-only data source".into(),
                    ));
                }
                let reply = self
                    .command(&db, doc! { "aggregate": coll, "pipeline": pipeline, "cursor": {} })
                    .await?;
                self.cursor_rows(reply, max_rows).await
            }
            "countDocuments" => {
                let pipeline = vec![
                    Bson::Document(doc! { "$match": document(0)? }),
                    Bson::Document(doc! { "$group": { "_id": 1, "n": { "$sum": 1 } } }),
                ];
                let reply = self
                    .command(&db, doc! { "aggregate": coll, "pipeline": pipeline, "cursor": {} })
                    .await?;
                let rs = self.cursor_rows(reply, 1).await?;
                let n = rs
                    .rows
                    .first()
                    .and_then(|r| r.get(rs.columns.iter().position(|c| c == "n")?))
                    .cloned()
                    .unwrap_or(Value::Int(0));
                Ok(single("count", n))
            }
            "estimatedDocumentCount" | "count" => {
                let reply =
                    self.command(&db, doc! { "count": coll, "query": document(0)? }).await?;
                Ok(single("count", reply.get("n").map_or(Value::Null, to_value)))
            }
            "distinct" => {
                let key = match arg(0) {
                    Some(Bson::String(k)) => k,
                    _ => return Err(Error::Query("distinct() needs a field name".into())),
                };
                let reply = self
                    .command(
                        &db,
                        doc! { "distinct": coll, "key": key.clone(), "query": document(1)? },
                    )
                    .await?;
                let rows = reply
                    .get_array("values")
                    .map(|a| a.iter().map(|v| vec![to_value(v)]).collect())
                    .unwrap_or_default();
                Ok(ResultSet { columns: vec![key], rows, ..ResultSet::default() })
            }
            "insertOne" | "insertMany" => {
                self.guard("insert")?;
                let mut docs: Vec<Bson> = match (method, arg(0)) {
                    ("insertOne", Some(d @ Bson::Document(_))) => vec![d],
                    ("insertMany", Some(Bson::Array(a))) => a,
                    _ => {
                        return Err(Error::Query(format!(
                            "{method}() needs {}",
                            if method == "insertOne" {
                                "a document"
                            } else {
                                "an array of documents"
                            }
                        )));
                    }
                };
                for d in &mut docs {
                    if let Bson::Document(d) = d {
                        if !d.contains_key("_id") {
                            let mut with_id = doc! { "_id": ObjectId::new() };
                            with_id.extend(std::mem::take(d));
                            *d = with_id;
                        }
                    }
                }
                Ok(write(
                    self.command(&db, doc! { "insert": coll, "documents": docs }).await?,
                    &["n"],
                ))
            }
            "updateOne" | "updateMany" | "replaceOne" => {
                self.guard("update")?;
                let update = match arg(1) {
                    Some(u @ (Bson::Document(_) | Bson::Array(_))) => u,
                    _ => {
                        return Err(Error::Query(format!(
                            "{method}() needs a filter and an update"
                        )));
                    }
                };
                let upsert = args
                    .get(2)
                    .and_then(Bson::as_document)
                    .and_then(|o| o.get_bool("upsert").ok())
                    .unwrap_or(false);
                let spec = doc! { "q": document(0)?, "u": update, "multi": method == "updateMany", "upsert": upsert };
                Ok(write(
                    self.command(&db, doc! { "update": coll, "updates": [spec] }).await?,
                    &["nModified", "n"],
                ))
            }
            "deleteOne" | "deleteMany" => {
                self.guard("delete")?;
                let spec = doc! { "q": document(0)?, "limit": i32::from(method == "deleteOne") };
                Ok(write(
                    self.command(&db, doc! { "delete": coll, "deletes": [spec] }).await?,
                    &["n"],
                ))
            }
            "drop" => {
                self.guard("drop")?;
                self.command(&db, doc! { "drop": coll }).await?;
                Ok(ResultSet::default())
            }
            "createIndex" => {
                self.guard("createIndexes")?;
                let keys = document(0)?;
                let mut spec = doc! { "key": keys.clone(), "name": index_name(&keys) };
                spec.extend(args.get(1).and_then(Bson::as_document).cloned().unwrap_or_default());
                self.command(&db, doc! { "createIndexes": coll, "indexes": [spec] }).await?;
                Ok(single("index", Value::Text(index_name(&keys))))
            }
            "getIndexes" => {
                let reply = self.command(&db, doc! { "listIndexes": coll, "cursor": {} }).await?;
                self.cursor_rows(reply, max_rows).await
            }
            other => Err(Error::Query(format!(
                "{other}() is not supported; run a command with db.runCommand({{...}})"
            ))),
        }
    }
}

fn first_document(args: &[Bson], what: &str) -> Result<Document> {
    match args.first() {
        Some(Bson::Document(d)) => Ok(d.clone()),
        _ => Err(Error::Query(format!("{what}() needs a command document"))),
    }
}

fn single(column: &str, v: Value) -> ResultSet {
    ResultSet { columns: vec![column.into()], rows: vec![vec![v]], ..ResultSet::default() }
}

fn index_name(keys: &Document) -> String {
    keys.iter()
        .map(|(k, v)| {
            format!(
                "{k}_{}",
                v.as_i32()
                    .map_or_else(|| to_json(v).trim_matches('"').to_owned(), |n| n.to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn as_i64(b: &Bson) -> Option<i64> {
    match b {
        Bson::Int32(n) => Some(i64::from(*n)),
        Bson::Int64(n) => Some(*n),
        Bson::Double(d) => Some(*d as i64),
        _ => None,
    }
}

fn as_u64(b: &Bson) -> Option<u64> {
    as_i64(b).and_then(|n| u64::try_from(n).ok())
}

/// Shell-style extended JSON for values that have no flat cell form.
fn to_json(b: &Bson) -> String {
    b.clone().into_canonical_extjson().to_string()
}

fn to_value(b: &Bson) -> Value {
    match b {
        Bson::Null | Bson::Undefined => Value::Null,
        Bson::Boolean(v) => Value::Bool(*v),
        Bson::Int32(n) => Value::Int(i64::from(*n)),
        Bson::Int64(n) => Value::Int(*n),
        Bson::Double(d) => Value::Float(*d),
        Bson::String(s) => Value::Text(s.clone()),
        Bson::ObjectId(o) => Value::Text(o.to_hex()),
        Bson::DateTime(d) => Value::Text(
            d.try_to_rfc3339_string().unwrap_or_else(|_| d.timestamp_millis().to_string()),
        ),
        Bson::Binary(bin) => Value::Bytes(bin.bytes.clone()),
        Bson::Decimal128(d) => Value::Text(d.to_string()),
        other => Value::Text(to_json(other)),
    }
}

/// One column per top-level key, in first-seen order; missing keys are NULL.
fn documents_to_result(docs: &[Document]) -> ResultSet {
    let mut columns: Vec<String> = Vec::new();
    for d in docs {
        for k in d.keys() {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let rows = docs
        .iter()
        .map(|d| columns.iter().map(|c| d.get(c).map_or(Value::Null, to_value)).collect())
        .collect();
    ResultSet { columns, rows, ..ResultSet::default() }
}

/// Columns from sampled documents. `sample` was already flattened to text cells, so the type of
/// each field is recovered from the cell's shape: ints stay ints, objects/arrays are JSON text.
fn infer_columns(sample: &ResultSet) -> Vec<Column> {
    sample
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let kinds: std::collections::BTreeSet<&str> = sample
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|v| match v {
                    Value::Null => "null",
                    Value::Bool(_) => "bool",
                    Value::Int(_) => "int",
                    Value::Float(_) => "double",
                    Value::Bytes(_) => "binData",
                    Value::Text(t) if t.starts_with('{') => "object",
                    Value::Text(t) if t.starts_with('[') => "array",
                    // ObjectIds were flattened to their 24 hex digits.
                    Value::Text(t) if t.len() == 24 && t.bytes().all(|b| b.is_ascii_hexdigit()) => {
                        "objectId"
                    }
                    Value::Text(_) => "string",
                })
                .collect();
            let present: Vec<&&str> = kinds.iter().filter(|k| **k != "null").collect();
            let native_type = match present.as_slice() {
                [] => "null".to_owned(),
                [one] => (**one).to_owned(),
                many => many.iter().map(|k| **k).collect::<Vec<_>>().join("|"),
            };
            Column { name: name.clone(), native_type, nullable: name != "_id", ..Column::default() }
        })
        .collect()
}

fn indexes(rs: &ResultSet) -> Vec<Index> {
    let at = |n: &str| rs.columns.iter().position(|c| c == n);
    let (Some(name), Some(key)) = (at("name"), at("key")) else { return Vec::new() };
    let unique = at("unique");
    rs.rows
        .iter()
        .filter_map(|r| {
            let Value::Text(n) = r.get(name)? else { return None };
            let Value::Text(k) = r.get(key)? else { return None };
            let keys = shell::parse_document(k).ok()?;
            Some(Index {
                name: n.clone(),
                unique: unique.is_some_and(|u| matches!(r.get(u), Some(Value::Bool(true))))
                    || n == "_id_",
                primary: n == "_id_",
                method: None,
                columns: keys.keys().cloned().collect(),
                predicate: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct CatalogConnection {
        database: String,
        fail: bool,
    }
    #[async_trait]
    impl Connection for CatalogConnection {
        async fn query_limited(&mut self, statement: &str, _max_rows: usize) -> Result<ResultSet> {
            match statement {
                "db.getName()" => Ok(single("database", Value::Text(self.database.clone()))),
                "show dbs" => Ok(ResultSet {
                    columns: vec!["name".into()],
                    rows: vec![
                        vec![Value::Text("first".into())],
                        vec![Value::Text("second".into())],
                    ],
                    ..ResultSet::default()
                }),
                "show collections" if self.fail => Err(Error::Query("Introspection denied".into())),
                "show collections" => Ok(ResultSet::default()),
                _ if statement.starts_with("use ") => {
                    self.database = statement[4..].into();
                    Ok(ResultSet::default())
                }
                _ => Err(Error::Query("Unexpected fixture command".into())),
            }
        }
    }
    #[tokio::test]
    async fn introspection_restores_the_configured_database_even_when_discovery_fails() {
        for fail in [false, true] {
            let mut connection = CatalogConnection { database: "communitiesDb".into(), fail };
            let result =
                MongoDialect.introspect(&mut connection, &IntrospectScope::default()).await;
            assert_eq!(connection.database, "communitiesDb");
            if !fail {
                assert_eq!(result.unwrap().default_schema.as_deref(), Some("communitiesDb"));
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn nested_bson_is_serialized_as_valid_extended_json() {
        let value = Bson::Document(
            doc! {"id": Bson::ObjectId(mongodb::bson::oid::ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap())},
        );
        let text = to_json(&value);
        assert!(text.contains("\"$oid\""));
        assert!(!text.contains("ObjectId("));
    }

    fn cfg(extra: &str) -> DataSourceConfig {
        toml::from_str(&format!("name='m'\ndialect='mongo'\n{extra}")).unwrap()
    }

    #[test]
    fn documents_become_rows_with_union_of_keys() {
        let rs = documents_to_result(&[
            doc! { "_id": 1, "a": "x" },
            doc! { "_id": 2, "b": { "c": 1 }, "t": [1, 2] },
        ]);
        assert_eq!(rs.columns, ["_id", "a", "b", "t"]);
        assert_eq!(rs.rows[0][2], Value::Null);
        assert_eq!(rs.rows[1][2], Value::Text(r#"{"c":{"$numberInt":"1"}}"#.into()));
        assert_eq!(rs.rows[1][3], Value::Text(r#"[{"$numberInt":"1"},{"$numberInt":"2"}]"#.into()));
    }

    #[test]
    fn values_map_to_cells() {
        let oid = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
        assert_eq!(to_value(&Bson::ObjectId(oid)), Value::Text("507f1f77bcf86cd799439011".into()));
        assert_eq!(to_value(&Bson::Int64(5)), Value::Int(5));
        assert_eq!(to_value(&Bson::Null), Value::Null);
        assert!(
            matches!(to_value(&Bson::DateTime(mongodb::bson::DateTime::from_millis(0))), Value::Text(t) if t.starts_with("1970-01-01"))
        );
    }

    #[test]
    fn infers_field_types_from_a_sample() {
        let rs = documents_to_result(&[
            doc! { "_id": 1, "n": 1, "s": "a" },
            doc! { "_id": 2, "n": 2.5, "s": Bson::Null, "o": { "k": 1 } },
        ]);
        let cols = infer_columns(&rs);
        let ty = |n: &str| cols.iter().find(|c| c.name == n).unwrap().native_type.clone();
        assert_eq!(ty("_id"), "int");
        assert_eq!(ty("n"), "double|int");
        assert_eq!(ty("s"), "string");
        assert_eq!(ty("o"), "object");
        assert!(!cols[0].nullable);
    }

    #[test]
    fn index_names_follow_the_server_convention() {
        assert_eq!(index_name(&doc! { "a": 1, "b": -1 }), "a_1_b_-1");
        assert_eq!(index_name(&doc! { "t": "text" }), "t_text");
    }

    #[tokio::test]
    async fn host_may_be_a_full_connection_string_and_tls_follows_ssl_mode() {
        let o =
            client_options(&cfg("host='mongodb://u:p@h1:1,h2:2/?replicaSet=rs0'")).await.unwrap();
        assert_eq!(o.hosts.len(), 2);
        assert!(o.tls.is_none());
        let o = client_options(&cfg("host='db.example.com'\nssl_mode='disable'\nuser='me'"))
            .await
            .unwrap();
        assert!(matches!(o.tls, Some(Tls::Disabled)));
        assert_eq!(o.credential.unwrap().source.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn auto_tls_matches_standard_uri_for_remote_hosts() {
        for host in ["192.168.40.157", "db.example.com", "localhost"] {
            let fields = client_options(&cfg(&format!("host='{host}'\nport=27057"))).await.unwrap();
            let uri =
                client_options(&cfg(&format!("host='mongodb://{host}:27057'"))).await.unwrap();
            assert!(uri.tls.is_none());
            assert!(fields.tls.is_none(), "Auto unexpectedly enabled TLS for {host}");
        }
    }

    #[tokio::test]
    async fn explicit_tls_settings_are_preserved() {
        let uri = client_options(&cfg("host='mongodb://db.example.com/?tls=true'")).await.unwrap();
        assert!(matches!(uri.tls, Some(Tls::Enabled(_))));
        for mode in ["require", "verify-ca", "verify-full"] {
            let o = client_options(&cfg(&format!("host='192.168.40.157'\nssl_mode='{mode}'")))
                .await
                .unwrap();
            assert!(matches!(o.tls, Some(Tls::Enabled(_))));
        }
    }
}
