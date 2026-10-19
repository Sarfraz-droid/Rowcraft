//! The desktop app: a Tauri shell. The window is the web frontend in `../src`; everything it can
//! do is a command below that forwards to `rowcraft-ui-model`, where the logic is unit-tested.
//!
//! Commands that touch the network or the disk are `async` so they never block the UI thread. Tauri
//! runs them on a tokio runtime, which is what the database drivers need.

use std::sync::Arc;

use rowcraft_core::Engine;
use rowcraft_model::Catalog;
use rowcraft_ui_model::{
    AddedSource, Bootstrap, Draft, DriverInfo, FormDefaults, Paths, QueryOutput, SourceSummary,
    Workbench,
};
use tauri::{Manager, State};

mod ai_router;

#[tauri::command]
async fn import_jdbc_jar(
    wb: State<'_, Workbench>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Choose a JDBC driver JAR file to upload.".into());
    };
    let directory = wb.jdbc_import_directory();
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || {
        rowcraft_ui_model::jdbc_file::import(&directory, &bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_sqlite(
    wb: State<'_, Workbench>,
    request: tauri::ipc::Request<'_>,
) -> Result<rowcraft_ui_model::sqlite_file::ImportedSqlite, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Choose a SQLite file to upload.".into());
    };
    let directory = wb.sqlite_import_directory();
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || {
        rowcraft_ui_model::sqlite_file::import(&directory, &bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn driver_settings(
    wb: State<'_, Workbench>,
) -> Result<rowcraft_ui_model::DriverSettings, String> {
    wb.driver_settings()
}

#[tauri::command]
async fn save_driver(
    wb: State<'_, Workbench>,
    driver: rowcraft_config::CustomDriverConfig,
) -> Result<Vec<DriverInfo>, String> {
    wb.save_driver(driver)
}

#[tauri::command]
async fn remove_driver(wb: State<'_, Workbench>, id: String) -> Result<Vec<DriverInfo>, String> {
    wb.remove_driver(&id)
}

#[tauri::command]
fn bootstrap(wb: State<'_, Workbench>) -> Bootstrap {
    wb.bootstrap()
}

#[tauri::command]
fn source_properties(
    wb: State<'_, Workbench>,
    name: String,
) -> Result<rowcraft_ui_model::source_properties::SourceProperties, String> {
    wb.source_properties(&name)
}

/// The whole driver catalog; the dialog filters it as the user types.
#[tauri::command]
fn drivers(wb: State<'_, Workbench>) -> Vec<DriverInfo> {
    wb.drivers("")
}

#[tauri::command]
fn form_defaults(wb: State<'_, Workbench>) -> FormDefaults {
    wb.form_defaults()
}

/// The draft after picking a driver: its dialect, and `localhost` as host if none was typed.
#[tauri::command]
fn select_driver(mut draft: Draft, id: String) -> Draft {
    draft.select_driver(&id);
    draft
}

#[tauri::command]
async fn add_source(wb: State<'_, Workbench>, draft: Draft) -> Result<AddedSource, String> {
    wb.add_source(&draft)
}

#[tauri::command]
fn edit_source(
    wb: State<'_, Workbench>,
    name: String,
) -> Result<rowcraft_ui_model::workbench::SourceEditDefaults, String> {
    wb.edit_source(&name)
}

#[tauri::command]
async fn update_source(
    wb: State<'_, Workbench>,
    original_name: String,
    draft: Draft,
) -> Result<AddedSource, String> {
    wb.update_source(&original_name, &draft)
}

#[tauri::command]
async fn test_connection(
    wb: State<'_, Workbench>,
    draft: Draft,
    original_name: Option<String>,
) -> Result<(), String> {
    wb.test_source_connection(&draft, original_name.as_deref()).await
}

#[tauri::command]
async fn save_ssh_config(wb: State<'_, Workbench>, draft: Draft) -> Result<Vec<String>, String> {
    wb.save_ssh_config(&draft)
}

#[tauri::command]
async fn connect(
    wb: State<'_, Workbench>,
    name: String,
    reconnect: Option<bool>,
) -> Result<String, String> {
    if reconnect.unwrap_or(false) { wb.reconnect(&name).await } else { wb.connect(&name).await }
}

#[tauri::command]
async fn run_query(
    wb: State<'_, Workbench>,
    statement: String,
    schema: Option<String>,
    show_all: Option<bool>,
) -> Result<QueryOutput, String> {
    wb.run_with_limit(&statement, schema.as_deref(), show_all.unwrap_or(false)).await
}

#[tauri::command]
fn remove_source(wb: State<'_, Workbench>, name: String) -> Result<Vec<SourceSummary>, String> {
    wb.remove_source(&name)
}

#[tauri::command]
async fn introspect(wb: State<'_, Workbench>) -> Result<Catalog, String> {
    wb.introspect().await
}

#[tauri::command]
fn preview_statement(
    wb: State<'_, Workbench>,
    schema: String,
    table: String,
) -> Result<String, String> {
    wb.preview_statement(&schema, &table)
}

#[tauri::command]
async fn ai_settings() -> Result<rowcraft_ui_model::ai::AiSettingsView, String> {
    tauri::async_runtime::spawn_blocking(|| {
        rowcraft_ui_model::ai::load(&rowcraft_ui_model::ai::settings_path())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_ai_settings(
    settings: rowcraft_ui_model::ai::AiSettings,
    api_key: Option<String>,
    clear_api_key: bool,
) -> Result<rowcraft_ui_model::ai::AiSettingsView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rowcraft_ui_model::ai::save(
            &rowcraft_ui_model::ai::settings_path(),
            settings,
            api_key,
            clear_api_key,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn ai_models(
    settings: rowcraft_ui_model::ai::AiSettings,
    api_key: Option<String>,
) -> Result<Vec<rowcraft_ui_model::ai::AiModel>, String> {
    rowcraft_ui_model::ai::models(
        &rowcraft_ui_model::ai::settings_path(),
        &settings,
        api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn ai_complete(
    messages: Vec<rowcraft_ui_model::ai::AiMessage>,
    tools_enabled: Option<bool>,
    model: Option<String>,
) -> Result<rowcraft_ui_model::ai::AiReply, String> {
    rowcraft_ui_model::ai::complete(
        &rowcraft_ui_model::ai::settings_path(),
        &messages,
        tools_enabled.unwrap_or(false),
        model.as_deref(),
    )
    .await
}

#[tauri::command]
async fn ai_read_schema(
    wb: State<'_, Workbench>,
    source: String,
    schema: Option<String>,
) -> Result<Catalog, String> {
    wb.ai_schema(&source, schema.as_deref()).await
}

#[tauri::command]
async fn ai_readonly_query(
    wb: State<'_, Workbench>,
    source: String,
    schema: Option<String>,
    statement: String,
) -> Result<QueryOutput, String> {
    wb.ai_query(&source, schema.as_deref(), &statement).await
}

async fn router_status(
    router: Arc<ai_router::RouterManager>,
    base_url: Option<String>,
) -> Result<ai_router::RouterStatus, String> {
    let mut status = tauri::async_runtime::spawn_blocking(move || router.status())
        .await
        .map_err(|e| e.to_string())??;
    if !status.running {
        let mut settings =
            rowcraft_ui_model::ai::load(&rowcraft_ui_model::ai::settings_path())?.settings;
        if let Some(base_url) = base_url {
            settings.base_url = base_url;
        }
        if rowcraft_ui_model::ai::gateway_running(&settings).await? {
            status.running = true;
            status.managed = false;
            status.version = None;
            let base_url = rowcraft_ui_model::ai::normalize(&settings)?.base_url;
            let target = base_url.clone();
            status.pid = tauri::async_runtime::spawn_blocking(move || {
                ai_router::RouterManager::external_pid(&target)
            })
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(None);
            status.base_url = Some(base_url);
        }
    }
    Ok(status)
}

#[tauri::command]
async fn ai_router_status(
    router: State<'_, Arc<ai_router::RouterManager>>,
    base_url: Option<String>,
) -> Result<ai_router::RouterStatus, String> {
    router_status(Arc::clone(router.inner()), base_url).await
}

#[tauri::command]
async fn start_ai_router(
    router: State<'_, Arc<ai_router::RouterManager>>,
    base_url: String,
) -> Result<ai_router::RouterStatus, String> {
    let current = router_status(Arc::clone(router.inner()), Some(base_url.clone())).await?;
    if current.running {
        return Ok(current);
    }
    let router = Arc::clone(router.inner());
    tauri::async_runtime::spawn_blocking(move || router.start(&base_url))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop_ai_router(
    router: State<'_, Arc<ai_router::RouterManager>>,
    base_url: Option<String>,
) -> Result<ai_router::RouterStatus, String> {
    let router = Arc::clone(router.inner());
    let base_url = match base_url {
        Some(url) => url,
        None => {
            rowcraft_ui_model::ai::load(&rowcraft_ui_model::ai::settings_path())?.settings.base_url
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        if router.status()?.managed { router.stop() } else { router.stop_external(&base_url) }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_ai_dashboard(settings: rowcraft_ui_model::ai::AiSettings) -> Result<(), String> {
    let url = rowcraft_ui_model::ai::dashboard_url(&settings)?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("open").arg(&url).status();
        #[cfg(target_os = "windows")]
        let status = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .status();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let status = std::process::Command::new("xdg-open").arg(&url).status();
        status
            .map_err(|e| format!("Could not open the router dashboard: {e}"))?
            .success()
            .then_some(())
            .ok_or_else(|| "Could not open the router dashboard.".to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    let engine = Arc::new(Engine::new(rowcraft_registry::default_registry()));
    // Before anything reads the driver catalog.
    let driver_warnings = rowcraft_config::register_custom_drivers();
    let workbench = Workbench::open(engine, Paths::default(), driver_warnings);

    tauri::Builder::default()
        .manage(workbench)
        .manage(Arc::new(ai_router::RouterManager::new(
            rowcraft_config::config_dir().join("ai-router"),
        )))
        .invoke_handler(tauri::generate_handler![
            ai_read_schema,
            ai_readonly_query,
            ai_settings,
            save_ai_settings,
            ai_models,
            ai_complete,
            ai_router_status,
            start_ai_router,
            stop_ai_router,
            open_ai_dashboard,
            bootstrap,
            source_properties,
            edit_source,
            update_source,
            import_sqlite,
            import_jdbc_jar,
            drivers,
            driver_settings,
            save_driver,
            remove_driver,
            form_defaults,
            select_driver,
            add_source,
            test_connection,
            save_ssh_config,
            connect,
            run_query,
            remove_source,
            introspect,
            preview_statement,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Rowcraft desktop app")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let _ = app.state::<Arc<ai_router::RouterManager>>().stop();
            }
        });
}
