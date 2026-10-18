use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::process::ExitCode;

use dg_config::{
    CustomDriverConfig, DataSourceConfig, DataSourceFile, DriverFile, DriverTarget, SshConfig,
    SshFile, datasources_path, drivers_path, ssh_path,
};
use dg_core::Engine;
use dg_dialect::IntrospectScope;

use crate::{
    AddArgs, DriverAddArgs, IntrospectArgs, IntrospectFormat, QueryArgs, SshAddArgs, tree,
};

const EXIT_STATEMENT: u8 = 1;
const EXIT_CONNECTION: u8 = 2;
const EXIT_USAGE: u8 = 3;

fn fail(code: u8, message: impl std::fmt::Display) -> ExitCode {
    eprintln!("dg: {message}");
    ExitCode::from(code)
}

fn exit_code(e: &dg_core::Error) -> u8 {
    use dg_dialect::Error as D;
    match e {
        dg_core::Error::Config(_) => EXIT_USAGE,
        dg_core::Error::Ssh(_) => EXIT_CONNECTION,
        dg_core::Error::Dialect(D::Connect(_)) => EXIT_CONNECTION,
        dg_core::Error::Dialect(D::Query(_)) => EXIT_STATEMENT,
        dg_core::Error::Dialect(D::NotImplemented(_) | D::UnknownDialect(_)) => EXIT_USAGE,
    }
}

fn load() -> Result<DataSourceFile, ExitCode> {
    DataSourceFile::load(&datasources_path()).map_err(|e| fail(EXIT_USAGE, e))
}

pub fn dialects(engine: &Engine) -> ExitCode {
    for d in engine.registry().iter() {
        println!("{:<12} {}", d.id(), d.display_name());
    }
    ExitCode::SUCCESS
}

pub fn ds_list() -> ExitCode {
    let file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if file.datasources.is_empty() {
        eprintln!("no data sources yet; add one with `dg ds add`");
    }
    for d in &file.datasources {
        let host = d.host.as_deref().unwrap_or("-");
        let port = d.port.map_or_else(|| "-".to_owned(), |p| p.to_string());
        println!(
            "{:<16} {:<10} {host}:{port}/{} user={}",
            d.name,
            d.dialect,
            d.database.as_deref().unwrap_or("-"),
            d.user.as_deref().unwrap_or("-"),
        );
    }
    ExitCode::SUCCESS
}

pub fn ds_add(engine: &Engine, a: AddArgs) -> ExitCode {
    // The dialect comes from --dialect, else from the catalog driver, else PostgreSQL.
    let driver = match a.driver.as_deref() {
        None => None,
        Some(id) => match dg_drivers::find(id) {
            Some(d) => Some(d),
            None => return fail(EXIT_USAGE, format!("unknown driver `{id}`; see `dg drivers`")),
        },
    };
    let dialect = match (&a.dialect, driver) {
        (Some(d), _) => d.clone(),
        (None, Some(d)) => match d.dialect {
            Some(dialect) => dialect.to_owned(),
            None => {
                return fail(
                    EXIT_USAGE,
                    format!("{} has no pure-Rust driver yet; see `dg drivers`", d.name),
                );
            }
        },
        (None, None) => "pg".to_owned(),
    };
    if let Err(e) = engine.registry().get(&dialect) {
        return fail(EXIT_USAGE, e);
    }
    let mut file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let ds = DataSourceConfig {
        name: a.name.clone(),
        dialect,
        host: a.host,
        port: a.port,
        database: a.database,
        user: a.user,
        password_env: a.password_env,
        ssl_mode: a.ssl_mode,
        read_only: a.read_only,
        driver: a.driver,
        comment: a.comment,
        url: a.url,
        schema_pattern: a.schema_pattern,
        object_filter: a.object_filter,
        ssh_tunnel: a.ssh,
        ssh_local_port: a.ssh_local_port,
        options: dg_config::ConnectionOptions {
            startup_script: a.startup_script,
            ..Default::default()
        },
        properties: a.properties.into_iter().collect(),
        ..Default::default()
    };
    // Refuse settings the engine could not use, rather than save them and fail at connect time.
    let checked = ds
        .effective()
        .map(drop)
        .and_then(|()| ds.parsed_schema_pattern().map(drop))
        .and_then(|()| ds.parsed_object_filter().map(drop));
    if let Err(e) = checked {
        return fail(EXIT_USAGE, e);
    }
    // A tunnel name that is not in ssh.toml would only fail at connect time; catch the typo now.
    if let Some(name) = ds.ssh_tunnel.as_deref() {
        let known = SshFile::load(&ssh_path()).and_then(|f| f.get(name).map(drop));
        if let Err(e) = known {
            return fail(EXIT_USAGE, format!("{e}; add it with `dg ssh add`"));
        }
    }
    if let Err(e) = file.add(ds).and_then(|()| file.save(&datasources_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!("added `{}` to {}", a.name, datasources_path().display());
    ExitCode::SUCCESS
}

pub fn drivers(engine: &Engine) -> ExitCode {
    println!("{:<16} {:<34} {:<12} {:>6}  STATUS", "ID", "NAME", "DIALECT", "PORT");
    // A custom driver that is rejected when loaded never reaches this list; its warning was
    // already printed at startup.
    for d in dg_drivers::all() {
        let status = match d.dialect.map(|dialect| engine.registry().get(dialect)) {
            Some(Ok(dialect)) if dialect.connects() => "ready",
            Some(Ok(_)) => "registered, not implemented yet",
            Some(Err(_)) => "not in this build",
            None => "no Rust driver yet",
        };
        let status = if d.custom { format!("{status} (custom)") } else { status.to_owned() };
        println!(
            "{:<16} {:<34} {:<12} {:>6}  {status}",
            d.id,
            d.name,
            d.dialect.unwrap_or("-"),
            d.port.map_or_else(|| "-".to_owned(), |p| p.to_string()),
        );
    }
    ExitCode::SUCCESS
}

fn load_drivers() -> Result<DriverFile, ExitCode> {
    DriverFile::load(&drivers_path()).map_err(|e| fail(EXIT_USAGE, e))
}

pub fn drivers_add(engine: &Engine, a: DriverAddArgs) -> ExitCode {
    // The dialect must exist in this build, or the driver could never connect.
    if let Err(e) = engine.registry().get(&a.dialect) {
        return fail(EXIT_USAGE, format!("{e}; see `dg dialects`"));
    }
    let mut file = match load_drivers() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let driver = CustomDriverConfig {
        id: a.id.clone(),
        name: a.name,
        dialect: a.dialect,
        port: a.port,
        target: if a.file { DriverTarget::File } else { DriverTarget::Network },
        jdbc_class: a.jdbc_class,
        jdbc_jars: a.jdbc_jars,
        jdbc_url_template: a.jdbc_url_template,
    };
    if let Err(e) = file.add(driver).and_then(|()| file.save(&drivers_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!(
        "added driver `{}` to {}; use it with `dg ds add <name> --driver {}`",
        a.id,
        drivers_path().display(),
        a.id
    );
    ExitCode::SUCCESS
}

pub fn drivers_rm(id: &str) -> ExitCode {
    let mut file = match load_drivers() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if let Err(e) = file.remove(id).and_then(|_| file.save(&drivers_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!("removed driver `{id}`");
    ExitCode::SUCCESS
}

pub fn ds_rm(name: &str) -> ExitCode {
    let mut file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if let Err(e) = file.remove(name).and_then(|_| file.save(&datasources_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!("removed `{name}`");
    ExitCode::SUCCESS
}

pub async fn ds_test(engine: &Engine, name: &str) -> ExitCode {
    let file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let ds = match file.get(name) {
        Ok(d) => d,
        Err(e) => return fail(EXIT_USAGE, e),
    };
    match engine.connect(ds).await {
        Ok(_) => {
            println!("ok: connected to `{name}`");
            ExitCode::SUCCESS
        }
        Err(e) => fail(exit_code(&e), e),
    }
}

pub async fn query(engine: &Engine, args: QueryArgs) -> ExitCode {
    let file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let ds = match file.get(&args.datasource) {
        Ok(d) => d,
        Err(e) => return fail(EXIT_USAGE, e),
    };
    let statement = if args.statement == "-" {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            return fail(EXIT_USAGE, format!("cannot read stdin: {e}"));
        }
        buf
    } else {
        args.statement
    };

    let mut conn = match engine.connect(ds).await {
        Ok(c) => c,
        Err(e) => return fail(exit_code(&e), e),
    };
    let result = match conn.query(&statement).await {
        Ok(r) => r,
        Err(e) => return fail(EXIT_STATEMENT, e),
    };

    let written = match &args.output {
        Some(path) => File::create(path).and_then(|f| {
            let mut w = BufWriter::new(f);
            dg_io::write_result(&result, args.format, &mut w)?;
            w.flush()
        }),
        None => {
            let mut out = io::stdout().lock();
            dg_io::write_result(&result, args.format, &mut out)
        }
    };
    match written {
        Ok(()) => ExitCode::SUCCESS,
        // A closed pipe (`dg query … | head`) is not an error worth reporting.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => fail(EXIT_USAGE, format!("cannot write output: {e}")),
    }
}

pub async fn introspect(engine: &Engine, args: IntrospectArgs) -> ExitCode {
    let file = match load() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let ds = match file.get(&args.datasource) {
        Ok(d) => d,
        Err(e) => return fail(EXIT_USAGE, e),
    };
    let scope =
        IntrospectScope { schemas: (!args.schemas.is_empty()).then(|| args.schemas.clone()) };
    let catalog = match engine.introspect(ds, &scope).await {
        Ok(c) => c,
        Err(e) => return fail(exit_code(&e), e),
    };
    // A schema that is not there is almost always a typo; an empty success would read as
    // "this schema has no objects" to a script.
    let missing: Vec<String> = args
        .schemas
        .iter()
        .filter(|name| catalog.schema(name).is_none())
        .map(|name| format!("`{name}`"))
        .collect();
    if !missing.is_empty() {
        return fail(EXIT_USAGE, format!("schema not found: {}", missing.join(", ")));
    }
    let text = match args.format {
        IntrospectFormat::Tree => tree::render(&catalog),
        IntrospectFormat::Json => match serde_json::to_string_pretty(&catalog) {
            Ok(json) => json + "\n",
            Err(e) => return fail(EXIT_USAGE, format!("cannot encode catalog: {e}")),
        },
    };
    match io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => fail(EXIT_USAGE, format!("cannot write output: {e}")),
    }
}

fn load_ssh() -> Result<SshFile, ExitCode> {
    SshFile::load(&ssh_path()).map_err(|e| fail(EXIT_USAGE, e))
}

pub fn ssh_list() -> ExitCode {
    let file = match load_ssh() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if file.configs.is_empty() {
        eprintln!("no SSH configurations yet; add one with `dg ssh add`");
    }
    for c in &file.configs {
        println!(
            "{:<16} {}@{}:{} auth={:?} host-key={:?}",
            c.name,
            c.user,
            c.host,
            c.port_or_default(),
            c.auth,
            c.host_key
        );
    }
    ExitCode::SUCCESS
}

pub fn ssh_add(a: SshAddArgs) -> ExitCode {
    let config = SshConfig {
        name: a.name.clone(),
        host: a.host,
        port: a.port,
        user: a.user,
        auth: a.auth,
        key_file: a.key_file,
        password_env: a.password_env,
        passphrase_env: a.passphrase_env,
        host_key: a.host_key,
        known_hosts_file: a.known_hosts_file,
        ..Default::default()
    };
    if let Err(message) = config.validate() {
        return fail(EXIT_USAGE, message);
    }
    let mut file = match load_ssh() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if let Err(e) = file.add(config).and_then(|()| file.save(&ssh_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!("added SSH configuration `{}` to {}", a.name, ssh_path().display());
    ExitCode::SUCCESS
}

pub fn ssh_rm(name: &str) -> ExitCode {
    let mut file = match load_ssh() {
        Ok(f) => f,
        Err(code) => return code,
    };
    if let Err(e) = file.remove(name).and_then(|_| file.save(&ssh_path())) {
        return fail(EXIT_USAGE, e);
    }
    println!("removed SSH configuration `{name}`");
    ExitCode::SUCCESS
}

/// Connects and authenticates; the tunnel to the SSH host's own port is opened and dropped.
pub async fn ssh_test(name: &str) -> ExitCode {
    use dg_ssh::{EnvCredentials, RusshOpener, Target, TunnelOpener};

    let file = match load_ssh() {
        Ok(f) => f,
        Err(code) => return code,
    };
    let config = match file.get(name) {
        Ok(c) => c.clone(),
        Err(e) => return fail(EXIT_USAGE, e),
    };
    let target = Target { host: config.host.clone(), port: config.port_or_default() };
    match RusshOpener.open(&config, &EnvCredentials, &[target], None).await {
        Ok(_) => {
            println!("ok: authenticated to `{name}` ({}@{})", config.user, config.host);
            ExitCode::SUCCESS
        }
        Err(e) => fail(EXIT_CONNECTION, e),
    }
}
