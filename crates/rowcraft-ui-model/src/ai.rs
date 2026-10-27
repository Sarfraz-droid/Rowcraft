//! AI gateway configuration and OpenAI-compatible routing. OAuth stays in the router;
//! only its gateway key is stored here, using the existing OS credential store.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, PoisonError},
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode, Url};
use rowcraft_config::passwords::{self, PendingSecrets};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub base_url: String,
    pub model: String,
    pub fallback_models: Vec<String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:20128/v1".into(),
            model: String::new(),
            fallback_models: vec![],
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    #[serde(flatten)]
    settings: AiSettings,
    api_key_account: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AiSettingsView {
    #[serde(flatten)]
    pub settings: AiSettings,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiModel {
    pub id: String,
    #[serde(default)]
    pub owned_by: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AiToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: AiToolFunction,
}

#[derive(Debug, Serialize)]
pub struct AiReply {
    pub content: String,
    pub model: String,
    pub tool_calls: Vec<AiToolCall>,
}

pub fn settings_path() -> PathBuf {
    rowcraft_config::config_dir().join("ai-settings.json")
}

fn read(path: &Path) -> Result<StoredSettings, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "AI settings could not be read. Check ai-settings.json.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StoredSettings::default()),
        Err(error) => Err(format!("Could not read AI settings: {error}")),
    }
}

pub fn load(path: &Path) -> Result<AiSettingsView, String> {
    let stored = read(path)?;
    Ok(AiSettingsView { settings: stored.settings, has_api_key: stored.api_key_account.is_some() })
}

/// Reuse an enabled key from the bundled router without copying its secret into settings.
/// Never attach local credentials to a remote gateway or an unrelated local port/path.
fn local_router_key(settings_path: &Path, base_url: &str) -> Option<String> {
    let url = Url::parse(base_url).ok()?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        || url.port() != Some(20128)
        || url.path().trim_end_matches('/') != "/v1"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let database = settings_path.parent()?.join("ai-router/data/db/data.sqlite");
    let connection = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .query_row(
            "SELECT key FROM apiKeys WHERE isActive = 1 AND coalesce(accessRestricted, 0) = 0 AND length(trim(key)) > 0 ORDER BY createdAt DESC, id LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

pub fn normalize(settings: &AiSettings) -> Result<AiSettings, String> {
    let mut url = Url::parse(settings.base_url.trim())
        .map_err(|_| "Enter a valid AI gateway URL.".to_owned())?;
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    let loopback =
        host == "localhost" || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err("Use HTTPS for remote gateways, or HTTP for a local router.".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Keep credentials and query parameters out of the gateway URL. Use the API key field."
                .into(),
        );
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(if path.is_empty() { "/v1" } else { &path });
    let model = settings.model.trim().to_owned();
    if model.len() > 256 || settings.fallback_models.len() > 5 {
        return Err("Use a model ID under 256 characters and at most five fallback models.".into());
    }
    let mut fallbacks = Vec::new();
    for item in &settings.fallback_models {
        let item = item.trim();
        if item.len() > 256 {
            return Err("A fallback model ID is too long.".into());
        }
        if !item.is_empty() && item != model && !fallbacks.iter().any(|value| value == item) {
            fallbacks.push(item.to_owned());
        }
    }
    Ok(AiSettings {
        base_url: url.to_string().trim_end_matches('/').to_owned(),
        model,
        fallback_models: fallbacks,
    })
}

pub fn save(
    path: &Path,
    settings: AiSettings,
    api_key: Option<String>,
    clear_api_key: bool,
) -> Result<AiSettingsView, String> {
    let settings = normalize(&settings)?;
    let previous = read(path)?;
    let mut pending = PendingSecrets::default();
    let key = api_key.as_deref().map(str::trim).filter(|key| !key.is_empty());
    let account = if let Some(key) = key {
        Some(pending.store("ai-gateway", key)?)
    } else if clear_api_key {
        None
    } else if previous.settings.base_url == settings.base_url {
        previous.api_key_account.clone()
    } else {
        None
    };
    let stored = StoredSettings { settings, api_key_account: account };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create AI settings directory: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(&stored).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("Could not save AI settings: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("Could not save AI settings: {error}"))?;
    pending.commit();
    if let Some(old) =
        previous.api_key_account.filter(|old| Some(old) != stored.api_key_account.as_ref())
    {
        let _ = passwords::delete(&old);
    }
    Ok(AiSettingsView { has_api_key: stored.api_key_account.is_some(), settings: stored.settings })
}

fn client() -> Result<Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .timeout(Duration::from_secs(45))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())
}

fn key(path: &Path, entered: Option<&str>, base_url: &str) -> Result<Option<String>, String> {
    if let Some(key) = entered.map(str::trim).filter(|key| !key.is_empty()) {
        return Ok(Some(key.to_owned()));
    }
    let stored = read(path)?;
    if stored.settings.base_url == base_url {
        if let Some(account) = stored.api_key_account.as_deref() {
            return passwords::load(account).map(Some);
        }
    }
    Ok(local_router_key(path, base_url))
}

fn authorized(request: reqwest::RequestBuilder, key: Option<&str>) -> reqwest::RequestBuilder {
    if let Some(key) = key { request.bearer_auth(key) } else { request }
}

async fn json_response<T: DeserializeOwned>(mut response: reqwest::Response) -> Result<T, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) =
        response.chunk().await.map_err(|_| "AI gateway response was interrupted.".to_owned())?
    {
        if bytes.len() + chunk.len() > 2_000_000 {
            return Err("AI gateway response exceeded the 2 MB limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "The AI gateway returned an unsupported response.".into())
}

/// A gateway that requires a key is still running. Probe its API rather than a bare port.
pub async fn gateway_running(settings: &AiSettings) -> Result<bool, String> {
    let settings = normalize(settings)?;
    let client = client()?;
    let response = match client
        .get(format!("{}/models", settings.base_url))
        .timeout(Duration::from_secs(3))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return Ok(false),
    };
    if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&response.status()) {
        return Ok(true);
    }
    if !response.status().is_success() {
        return Ok(false);
    }
    let body: Value = json_response(response).await?;
    Ok(body.get("data").is_some_and(Value::is_array))
}

pub async fn models(
    path: &Path,
    settings: &AiSettings,
    entered_key: Option<&str>,
) -> Result<Vec<AiModel>, String> {
    let settings = normalize(settings)?;
    let key = key(path, entered_key, &settings.base_url)?;
    let response =
        authorized(client()?.get(format!("{}/models", settings.base_url)), key.as_deref())
            .send()
            .await
            .map_err(|_| {
                "Could not reach the AI gateway. Start 9router or check its URL.".to_owned()
            })?;
    if !response.status().is_success() {
        return Err(format!(
            "AI gateway returned HTTP {}. Check its URL and API key.",
            response.status().as_u16()
        ));
    }
    #[derive(Deserialize)]
    struct Models {
        data: Vec<AiModel>,
    }
    let mut models = json_response::<Models>(response).await?.data;
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

pub async fn complete(
    path: &Path,
    messages: &[AiMessage],
    tools_enabled: bool,
    model: Option<&str>,
) -> Result<AiReply, String> {
    let stored = read(path)?;
    let mut requested = stored.settings;
    if let Some(model) = model {
        requested.model = model.to_owned();
    }
    let settings = normalize(&requested)?;
    if settings.model.is_empty() {
        return Err("Choose a model or routing combo in AI settings first.".into());
    }
    if messages.is_empty()
        || messages.len() > 80
        || messages
            .iter()
            .any(|m| !["system", "user", "assistant", "tool"].contains(&m.role.as_str()))
    {
        return Err("The AI conversation is invalid or too long.".into());
    }
    if serde_json::to_vec(messages).map_err(|e| e.to_string())?.len() > 100_000 {
        return Err("Keep the AI conversation below 100 KB.".into());
    }
    let key = key(path, None, &settings.base_url)?;
    let client = client()?;
    let spare = spare_models(&client, &settings, key.as_deref()).await;
    complete_with(&client, &route(&settings, spare), key.as_deref(), messages, tools_enabled).await
}

/// How long a model that just failed waits at the back of the route.
const COOLDOWN: Duration = Duration::from_secs(120);
/// Models that failed recently, so the next request does not wait on them first again.
static COOLING: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
/// The gateway's model list, fetched at most every ten minutes.
static LISTED: LazyLock<Mutex<Option<Listed>>> = LazyLock::new(Default::default);

#[derive(Debug, Clone)]
struct Listed {
    base_url: String,
    fetched: Instant,
    ids: Vec<String>,
}

fn cool(model: &str) {
    COOLING.lock().unwrap_or_else(PoisonError::into_inner).insert(model.to_owned(), Instant::now());
}

fn warm(model: &str) {
    COOLING.lock().unwrap_or_else(PoisonError::into_inner).remove(model);
}

fn cooling(model: &str) -> bool {
    COOLING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(model)
        .is_some_and(|at| at.elapsed() < COOLDOWN)
}

/// Whether a listed model id looks like a chat model rather than embeddings, speech or images.
fn chat_capable(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    ![
        "embed",
        "tts",
        "whisper",
        "transcri",
        "speech",
        "audio",
        "image",
        "dall-e",
        "moderation",
        "rerank",
        "realtime",
    ]
    .iter()
    .any(|word| id.contains(word))
}

/// Up to two gateway models outside the configured route, tried only after it fails: first those
/// from the primary model's provider (`provider/model`), then the gateway's order.
fn pick_spares(settings: &AiSettings, listed: &[String]) -> Vec<String> {
    let provider = settings.model.split_once('/').map(|(provider, _)| provider);
    let mut spare: Vec<&String> = listed
        .iter()
        .filter(|id| {
            chat_capable(id) && **id != settings.model && !settings.fallback_models.contains(id)
        })
        .collect();
    spare.sort_by_key(|id| {
        provider.is_none_or(|p| id.split_once('/').map(|(own, _)| own) != Some(p))
    });
    spare.into_iter().take(2).cloned().collect()
}

async fn spare_models(client: &Client, settings: &AiSettings, key: Option<&str>) -> Vec<String> {
    let cached = LISTED.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let listed = match cached {
        Some(listed)
            if listed.base_url == settings.base_url
                && listed.fetched.elapsed() < Duration::from_secs(600) =>
        {
            listed.ids
        }
        _ => {
            let request = authorized(client.get(format!("{}/models", settings.base_url)), key)
                .timeout(Duration::from_secs(5));
            #[derive(Deserialize)]
            struct Models {
                data: Vec<AiModel>,
            }
            let ids = match request.send().await {
                Ok(response) if response.status().is_success() => json_response::<Models>(response)
                    .await
                    .map(|models| models.data.into_iter().map(|model| model.id).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            *LISTED.lock().unwrap_or_else(PoisonError::into_inner) = Some(Listed {
                base_url: settings.base_url.clone(),
                fetched: Instant::now(),
                ids: ids.clone(),
            });
            ids
        }
    };
    pick_spares(settings, &listed)
}

/// The models to try in order: the configured route, then spares, with recently failed models
/// moved to the back (still tried, in case everything else is down too).
fn route(settings: &AiSettings, spare: Vec<String>) -> AiSettings {
    let mut models: Vec<String> = Vec::new();
    for model in std::iter::once(&settings.model).chain(&settings.fallback_models).chain(&spare) {
        if !models.contains(model) {
            models.push(model.clone());
        }
    }
    models.sort_by_key(|model| cooling(model));
    let mut models = models.into_iter();
    AiSettings {
        base_url: settings.base_url.clone(),
        model: models.next().unwrap_or_default(),
        fallback_models: models.collect(),
    }
}

async fn complete_with(
    client: &Client,
    settings: &AiSettings,
    key: Option<&str>,
    messages: &[AiMessage],
    tools_enabled: bool,
) -> Result<AiReply, String> {
    route_completion(settings, tools_enabled, |model, tools| {
        let mut body = json!({ "model": model, "messages": messages, "stream": false });
        if tools {
            body["tools"] = json!([
                {"type":"function","function":{"name":"read_schema","description":"Inspect table and column metadata for the active source and schema.","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
                {"type":"function","function":{"name":"run_readonly_query","description":"Run one SELECT query against the active database in a read-only transaction. At most 25 rows are returned. Use this to inspect samples or test a query. No mutations, file operations, or locking clauses.","parameters":{"type":"object","properties":{"sql":{"type":"string"}},"required":["sql"],"additionalProperties":false}}}
            ]);
            body["tool_choice"] = json!("auto");
        }
        let request = authorized(client.post(format!("{}/chat/completions", settings.base_url)), key).json(&body);
        async move {
            let response = match request.send().await {
                Ok(response) => response,
                // A slow model is that model's problem: move on to the next one.
                Err(error) if error.is_timeout() => {
                    return Ok(GatewayResponse { status: StatusCode::REQUEST_TIMEOUT, data: None });
                }
                Err(_) => {
                    return Err("Could not reach the AI gateway. Check its connection and try again.".to_owned());
                }
            };
            let status = response.status();
            let data = if status.is_success() {
                Some(json_response::<Value>(response).await?)
            } else {
                // Routers can wrap an upstream permission error in HTTP 503.
                // Keep structured details for classification, never display raw bodies.
                json_response::<Value>(response).await.ok()
            };
            Ok(GatewayResponse { status, data })
        }
    })
    .await
}

#[derive(Debug)]
struct GatewayResponse {
    status: StatusCode,
    data: Option<Value>,
}

fn provider_access_error(data: Option<&Value>) -> Option<u16> {
    let error = data?.get("error")?;
    let code = error.get("code").and_then(Value::as_u64);
    let message =
        error.get("message").and_then(Value::as_str).or_else(|| error.as_str()).unwrap_or_default();
    [401, 403].into_iter().find(|status| {
        code == Some(u64::from(*status))
            || message.contains(&format!("HTTP {status}"))
            || message.contains(&format!("[{status}]"))
    })
}

/// The model's message, or why it is unusable so the next model is tried.
fn parse_reply(model: &str, data: Option<Value>) -> Result<AiReply, &'static str> {
    let data = data.ok_or("no response data")?;
    let tool_calls: Vec<AiToolCall> = data
        .pointer("/choices/0/message/tool_calls")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| "invalid database tool calls")?
        .unwrap_or_default();
    let content =
        data.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or_default();
    if content.trim().is_empty() && tool_calls.is_empty() {
        return Err("empty reply");
    }
    Ok(AiReply {
        content: content.to_owned(),
        model: data.get("model").and_then(Value::as_str).unwrap_or(model).to_owned(),
        tool_calls,
    })
}

/// Tries each model in turn. Any failure of one model (rate limit, outage, timeout, a request it
/// rejects, an empty or malformed reply) moves on to the next; only a rejected gateway key stops,
/// since every model would be refused the same way.
async fn route_completion<F, Fut>(
    settings: &AiSettings,
    tools_enabled: bool,
    mut send: F,
) -> Result<AiReply, String>
where
    F: FnMut(String, bool) -> Fut,
    Fut: std::future::Future<Output = Result<GatewayResponse, String>>,
{
    let mut failures = Vec::new();
    let mut access_rejected = false;
    for model in std::iter::once(&settings.model).chain(&settings.fallback_models) {
        let mut response = send(model.clone(), tools_enabled).await?;
        if matches!(response.status.as_u16(), 502..=504)
            && provider_access_error(response.data.as_ref()).is_none()
        {
            // One short retry handles transient upstream failures without looping
            // on rejected credentials or a provider's account cooldown.
            tokio::time::sleep(Duration::from_millis(500)).await;
            response = send(model.clone(), tools_enabled).await?;
        }
        if tools_enabled && matches!(response.status.as_u16(), 400 | 422) {
            // Many models reject the `tools` field. Answering without database checks beats failing.
            response = send(model.clone(), false).await?;
        }
        let status = response.status;
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(format!(
                "AI gateway returned HTTP {}. Check its API key and model configuration.",
                status.as_u16()
            ));
        }
        if !status.is_success() {
            match provider_access_error(response.data.as_ref()).filter(|_| status.is_server_error())
            {
                Some(upstream) => {
                    access_rejected = true;
                    failures.push(format!("{model}: provider rejected access (HTTP {upstream})"));
                }
                None => failures.push(format!("{model} (HTTP {})", status.as_u16())),
            }
            cool(model);
            continue;
        }
        match parse_reply(model, response.data) {
            Ok(reply) => {
                warm(model);
                return Ok(reply);
            }
            Err(problem) => {
                failures.push(format!("{model}: {problem}"));
                cool(model);
            }
        }
    }
    let recovery = if access_rejected {
        "Reconnect the provider in the router dashboard and check model/project access, or choose another model in AI settings."
    } else {
        "Check the provider in the router dashboard or configure a fallback model in AI settings."
    };
    Err(format!("All configured AI routes are unavailable: {}. {recovery}", failures.join(", ")))
}

pub fn dashboard_url(settings: &AiSettings) -> Result<String, String> {
    let normalized = normalize(settings)?;
    Ok(format!("{}/dashboard", normalized.base_url.trim_end_matches("/v1")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rowcraft-ai-{}.json",
            passwords::new_account("test").expect("account ID").replace(':', "-")
        ))
    }

    #[test]
    fn reuses_only_active_unrestricted_local_router_keys() {
        let directory = fixture().with_extension("router");
        let database = directory.join("ai-router/data/db/data.sqlite");
        std::fs::create_dir_all(database.parent().expect("parent")).expect("fixture directory");
        let connection = rusqlite::Connection::open(&database).expect("database");
        connection.execute_batch(
            "CREATE TABLE apiKeys (id TEXT, key TEXT, isActive INTEGER, accessRestricted INTEGER, createdAt TEXT);
             INSERT INTO apiKeys VALUES ('enabled', 'fixture-enabled', 1, 0, '2026-01-01');
             INSERT INTO apiKeys VALUES ('disabled', 'fixture-disabled', 0, 0, '2026-01-03');
             INSERT INTO apiKeys VALUES ('restricted', 'fixture-restricted', 1, 1, '2026-01-04');",
        ).expect("fixture keys");
        let path = directory.join("ai-settings.json");
        for url in
            ["http://127.0.0.1:20128/v1", "http://localhost:20128/v1", "http://[::1]:20128/v1"]
        {
            assert_eq!(key(&path, None, url).expect("key").as_deref(), Some("fixture-enabled"));
        }
        for url in [
            "https://example.com/v1",
            "http://127.0.0.1:20129/v1",
            "http://127.0.0.1:20128/other",
            "http://user@localhost:20128/v1",
            "http://localhost:20128/v1?redirect=1",
        ] {
            assert!(key(&path, None, url).expect("key").is_none());
        }
        assert_eq!(
            key(&path, Some("entered-key"), "http://localhost:20128/v1")
                .expect("override")
                .as_deref(),
            Some("entered-key")
        );
        connection
            .execute("DELETE FROM apiKeys WHERE id = 'enabled'", [])
            .expect("remove enabled key");
        assert!(key(&path, None, "http://localhost:20128/v1").expect("no key").is_none());
        drop(connection);
        std::fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn normalizes_local_urls_and_deduplicates_fallbacks() {
        let settings = normalize(&AiSettings {
            base_url: " http://127.0.0.1:20128/ ".into(),
            model: " primary ".into(),
            fallback_models: vec![
                " primary ".into(),
                "free/backup".into(),
                "free/backup".into(),
                "".into(),
            ],
        })
        .expect("valid local settings");
        assert_eq!(settings.base_url, "http://127.0.0.1:20128/v1");
        assert_eq!(settings.fallback_models, ["free/backup"]);
    }

    #[test]
    fn rejects_insecure_remote_urls_and_embedded_credentials() {
        for url in [
            "http://example.com/v1",
            "https://user:secret@example.com/v1",
            "https://example.com/v1?key=secret",
            "file:///tmp/router",
        ] {
            assert!(normalize(&AiSettings { base_url: url.into(), ..Default::default() }).is_err());
        }
        assert!(
            normalize(&AiSettings {
                base_url: "http://[::1]:20128/v1".into(),
                ..Default::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn saves_and_loads_keyless_settings_without_plaintext_credentials() {
        let path = fixture();
        let saved =
            save(&path, AiSettings { model: "free-sql".into(), ..Default::default() }, None, false)
                .expect("save");
        assert!(!saved.has_api_key);
        assert_eq!(load(&path).expect("load").settings.model, "free-sql");
        let text = std::fs::read_to_string(&path).expect("settings file");
        assert!(!text.contains("api_key\""));
        std::fs::remove_file(path).expect("remove fixture");
    }

    #[tokio::test]
    async fn automatically_falls_back_on_rate_limits_and_returns_the_serving_model() {
        let settings = AiSettings {
            model: "preferred".into(),
            fallback_models: vec!["free/backup".into()],
            ..Default::default()
        };
        let mut sent = Vec::new();
        let reply = route_completion(&settings, false, |model, _| {
            sent.push(model.clone());
            std::future::ready(Ok(if model == "preferred" {
                GatewayResponse { status: StatusCode::TOO_MANY_REQUESTS, data: None }
            } else {
                GatewayResponse { status: StatusCode::OK, data: Some(json!({"model":"free/backup","choices":[{"message":{"content":"SELECT 1;"}}]})) }
            }))
        }).await.expect("fallback reply");
        assert_eq!(reply.content, "SELECT 1;");
        assert_eq!(reply.model, "free/backup");
        assert_eq!(sent, ["preferred", "free/backup"]);
    }

    #[tokio::test]
    async fn retries_a_temporary_503_before_reporting_the_route_unavailable() {
        let settings = AiSettings { model: "gc/gemini-2.5-flash".into(), ..Default::default() };
        let mut attempts = 0;
        let reply = route_completion(&settings, false, |_, _| {
            attempts += 1;
            std::future::ready(Ok(if attempts == 1 {
                GatewayResponse { status: StatusCode::SERVICE_UNAVAILABLE, data: None }
            } else {
                GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"SELECT 1;"}}]})),
                }
            }))
        })
        .await
        .expect("temporary 503 should recover");
        assert_eq!(reply.content, "SELECT 1;");
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn explains_provider_access_errors_wrapped_in_a_gateway_503_without_retrying() {
        let settings = AiSettings { model: "gc/gemini-2.5-flash".into(), ..Default::default() };
        let mut attempts = 0;
        let error = route_completion(&settings, false, |_, _| {
            attempts += 1;
            std::future::ready(Ok(GatewayResponse {
                status: StatusCode::SERVICE_UNAVAILABLE,
                data: Some(json!({"error":{"message":"[403]: HTTP 403 (reset after 55s)","type":"server_error"}})),
            }))
        }).await.expect_err("provider access failure");
        assert!(error.contains("provider rejected access (HTTP 403)"), "{error}");
        assert!(error.contains("Reconnect"), "{error}");
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn persistent_503_has_a_bounded_retry_then_uses_the_configured_fallback() {
        let settings = AiSettings {
            model: "unavailable".into(),
            fallback_models: vec!["backup".into()],
            ..Default::default()
        };
        let mut sent = Vec::new();
        let reply = route_completion(&settings, false, |model, _| {
            sent.push(model.clone());
            std::future::ready(Ok(if model == "unavailable" {
                GatewayResponse { status: StatusCode::SERVICE_UNAVAILABLE, data: None }
            } else {
                GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"SELECT 1;"}}]})),
                }
            }))
        })
        .await
        .expect("fallback reply");
        assert_eq!(reply.model, "backup");
        assert_eq!(sent, ["unavailable", "unavailable", "backup"]);
    }

    #[tokio::test]
    async fn provider_access_rejection_tries_the_configured_fallback_without_retrying() {
        let settings = AiSettings {
            model: "rejected".into(),
            fallback_models: vec!["backup".into()],
            ..Default::default()
        };
        let mut sent = Vec::new();
        let reply = route_completion(&settings, false, |model, _| {
            sent.push(model.clone());
            std::future::ready(Ok(if model == "rejected" {
                GatewayResponse {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    data: Some(json!({"error":{"message":"[403]: HTTP 403"}})),
                }
            } else {
                GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"SELECT 1;"}}]})),
                }
            }))
        })
        .await
        .expect("fallback reply");
        assert_eq!(reply.model, "backup");
        assert_eq!(sent, ["rejected", "backup"]);
    }

    #[tokio::test]
    async fn authentication_errors_stop_routing_instead_of_trying_every_model() {
        let settings = AiSettings {
            model: "preferred".into(),
            fallback_models: vec!["backup".into()],
            ..Default::default()
        };
        let mut sent = Vec::new();
        let error = route_completion(&settings, false, |model, _| {
            sent.push(model);
            std::future::ready(Ok(GatewayResponse { status: StatusCode::UNAUTHORIZED, data: None }))
        })
        .await
        .expect_err("authentication error");
        assert!(error.contains("401"));
        assert_eq!(sent, ["preferred"]);
    }

    #[tokio::test]
    async fn retries_without_tools_when_a_model_rejects_them() {
        let settings = AiSettings { model: "no-tools-model".into(), ..Default::default() };
        let mut sent = Vec::new();
        let reply = route_completion(&settings, true, |_, tools| {
            sent.push(tools);
            std::future::ready(Ok(if tools {
                GatewayResponse { status: StatusCode::BAD_REQUEST, data: None }
            } else {
                GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"SELECT 1;"}}]})),
                }
            }))
        })
        .await
        .expect("reply without tools");
        assert_eq!(reply.content, "SELECT 1;");
        assert_eq!(sent, [true, false]);
    }

    #[tokio::test]
    async fn empty_replies_timeouts_and_bad_requests_move_on_to_the_next_model() {
        let settings = AiSettings {
            model: "silent".into(),
            fallback_models: vec!["slow".into(), "confused".into(), "working".into()],
            ..Default::default()
        };
        let mut sent = Vec::new();
        let reply = route_completion(&settings, false, |model, _| {
            sent.push(model.clone());
            std::future::ready(Ok(match model.as_str() {
                "silent" => GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"  "}}]})),
                },
                "slow" => GatewayResponse { status: StatusCode::REQUEST_TIMEOUT, data: None },
                "confused" => GatewayResponse { status: StatusCode::BAD_REQUEST, data: None },
                _ => GatewayResponse {
                    status: StatusCode::OK,
                    data: Some(json!({"choices":[{"message":{"content":"SELECT 1;"}}]})),
                },
            }))
        })
        .await
        .expect("working model reply");
        assert_eq!(reply.model, "working");
        assert_eq!(sent, ["silent", "slow", "confused", "working"]);
    }

    #[test]
    fn spare_models_prefer_the_primary_provider_and_skip_non_chat_models() {
        let settings = AiSettings {
            model: "gc/gemini-2.5-pro".into(),
            fallback_models: vec!["gc/gemini-2.5-flash".into()],
            ..Default::default()
        };
        let listed = [
            "openai/gpt-4o-mini",
            "gc/text-embedding-004",
            "gc/gemini-2.5-pro",
            "gc/gemini-2.5-flash",
            "gc/gemini-2.0-flash",
            "openai/whisper-1",
        ]
        .map(String::from);
        assert_eq!(pick_spares(&settings, &listed), ["gc/gemini-2.0-flash", "openai/gpt-4o-mini"]);
    }

    #[test]
    fn recently_failed_models_move_to_the_back_of_the_route() {
        let settings = AiSettings {
            model: "route-test/down".into(),
            fallback_models: vec!["route-test/up".into()],
            ..Default::default()
        };
        cool("route-test/down");
        let routed = route(&settings, vec!["route-test/spare".into()]);
        assert_eq!(routed.model, "route-test/up");
        assert_eq!(routed.fallback_models, ["route-test/spare", "route-test/down"]);
        warm("route-test/down");
        assert_eq!(route(&settings, vec![]).model, "route-test/down");
    }

    #[test]
    fn gateway_key_is_only_attached_as_authorization_header() {
        let request = authorized(
            client().expect("client").get("http://localhost:20128/v1/models"),
            Some("fixture-only-key"),
        )
        .build()
        .expect("request");
        assert_eq!(request.headers()["authorization"], "Bearer fixture-only-key");
        assert!(!request.url().as_str().contains("fixture-only-key"));
    }
}
