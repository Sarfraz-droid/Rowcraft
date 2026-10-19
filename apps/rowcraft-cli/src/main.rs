mod commands;
mod tree;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use rowcraft_config::{HostKeyPolicy, SshAuth, SslMode};
use rowcraft_io::Format;

/// Exit codes: 0 ok, 1 statement error, 2 connection/auth error, 3 usage or config error.
#[derive(Debug, Parser)]
#[command(name = "rowcraft", version, about = "Rowcraft command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List the databases this build supports.
    Dialects,
    /// List the built-in databases and their available execution dialects.
    /// `add` and `rm` manage your own entries for databases that speak a protocol we implement.
    Drivers {
        #[command(subcommand)]
        action: Option<DriversCommand>,
    },
    /// Manage saved data sources.
    #[command(subcommand)]
    Ds(DsCommand),
    /// Run a statement and print the result.
    Query(QueryArgs),
    /// Manage shared SSH configurations used by SSH tunnels.
    #[command(subcommand)]
    Ssh(SshCommand),
    /// Read the database catalog (schemas, tables, columns, indexes, keys, routines).
    Introspect(IntrospectArgs),
}

#[derive(Debug, Subcommand)]
enum DriversCommand {
    /// List the catalog (the default).
    List,
    /// Add a custom driver: a catalog entry that connects through one of our dialects.
    Add(DriverAddArgs),
    /// Remove a custom driver.
    Rm { id: String },
}

#[derive(Debug, Args)]
struct DriverAddArgs {
    /// Catalog id: a-z, 0-9, `_`, `-`. Used with `rowcraft ds add --driver <id>`.
    id: String,
    /// Display name.
    #[arg(long)]
    name: String,
    /// Dialect that does the talking (see `rowcraft dialects`), e.g. `pg` for a PostgreSQL-compatible server.
    #[arg(long)]
    dialect: String,
    /// Default port.
    #[arg(long)]
    port: Option<u16>,
    /// The database is a file or directory path instead of host and port.
    #[arg(long)]
    file: bool,
    /// JDBC driver class (use --dialect jdbc).
    #[arg(long)]
    jdbc_class: Option<String>,
    /// JDBC library path; repeat for dependencies.
    #[arg(long = "jdbc-jar")]
    jdbc_jars: Vec<std::path::PathBuf>,
    /// JDBC URL template, with {host}, {port}, {database} placeholders.
    #[arg(long)]
    jdbc_url_template: Option<String>,
}

#[derive(Debug, Subcommand)]
enum SshCommand {
    /// List SSH configurations.
    List,
    /// Add an SSH configuration. Secrets are never stored: name environment variables instead.
    Add(SshAddArgs),
    /// Remove an SSH configuration.
    Rm { name: String },
    /// Connect and authenticate, then report success or the error.
    Test { name: String },
}

#[derive(Debug, Args)]
struct SshAddArgs {
    name: String,
    #[arg(long)]
    host: String,
    /// SSH port; default 22.
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    user: String,
    /// password, key-pair or agent.
    #[arg(long, value_parser = parse_ssh_auth, default_value = "password")]
    auth: SshAuth,
    /// Private key file (key-pair authentication).
    #[arg(long)]
    key_file: Option<String>,
    /// Environment variable holding the password.
    #[arg(long)]
    password_env: Option<String>,
    /// Environment variable holding the key passphrase.
    #[arg(long)]
    passphrase_env: Option<String>,
    /// strict (default: refuse hosts not in known_hosts) or accept-new (trust and remember).
    #[arg(long, value_parser = parse_host_key, default_value = "strict")]
    host_key: HostKeyPolicy,
    /// Defaults to ~/.ssh/known_hosts.
    #[arg(long)]
    known_hosts_file: Option<String>,
}

#[derive(Debug, Subcommand)]
enum DsCommand {
    /// List data sources.
    List,
    /// Add a data source. Passwords are never stored: name an environment variable instead.
    Add(Box<AddArgs>),
    /// Remove a data source.
    Rm { name: String },
    /// Connect and report success or the error.
    Test { name: String },
}

#[derive(Debug, Args)]
struct AddArgs {
    name: String,
    /// Database from the catalog (see `rowcraft drivers`); sets the dialect.
    #[arg(long)]
    driver: Option<String>,
    /// Dialect id; see `rowcraft dialects`. Defaults to the driver's dialect, else `pg`.
    #[arg(long)]
    dialect: Option<String>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    database: Option<String>,
    #[arg(long)]
    user: Option<String>,
    /// Environment variable holding the password.
    #[arg(long)]
    password_env: Option<String>,
    /// auto (TLS unless the host is loopback), disable, require, verify-ca or verify-full.
    #[arg(long, value_parser = parse_ssl_mode, default_value = "auto")]
    ssl_mode: SslMode,
    /// Open sessions read-only where the database supports it.
    #[arg(long)]
    read_only: bool,
    /// scheme://host:port/database; overrides host, port, database and user.
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    comment: Option<String>,
    /// Statements to run right after connecting.
    #[arg(long)]
    startup_script: Option<String>,
    /// Which schemas to show, e.g. `*:public` or `@:@|db1:s1,s2`.
    #[arg(long)]
    schema_pattern: Option<String>,
    /// Which objects to show, e.g. `table:-tmp_.*||view:v_.*`.
    #[arg(long)]
    object_filter: Option<String>,
    /// Name of a shared SSH configuration (see `rowcraft ssh add`) to tunnel through.
    #[arg(long)]
    ssh: Option<String>,
    /// Listen on this local port for the tunnel instead of a free one.
    #[arg(long)]
    ssh_local_port: Option<u16>,
    /// Driver property as name=value; repeat for several.
    #[arg(long = "property", value_parser = parse_property)]
    properties: Vec<(String, String)>,
}

#[derive(Debug, Args)]
struct QueryArgs {
    /// Data source name.
    #[arg(short, long)]
    datasource: String,
    /// SQL to run; `-` reads it from stdin.
    statement: String,
    /// table, csv, tsv, json, ndjson or md.
    #[arg(short, long, default_value = "table")]
    format: Format,
    /// Write to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct IntrospectArgs {
    /// Data source name.
    #[arg(short, long)]
    datasource: String,
    /// Only this schema; repeat for several. Default: every user schema.
    #[arg(short, long = "schema")]
    schemas: Vec<String>,
    /// tree or json.
    #[arg(short, long, default_value = "tree")]
    format: IntrospectFormat,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum IntrospectFormat {
    Tree,
    Json,
}

fn parse_ssl_mode(s: &str) -> Result<SslMode, String> {
    match s {
        "auto" => Ok(SslMode::Auto),
        "disable" => Ok(SslMode::Disable),
        "require" => Ok(SslMode::Require),
        "verify-ca" => Ok(SslMode::VerifyCa),
        "verify-full" => Ok(SslMode::VerifyFull),
        other => {
            Err(format!("`{other}`: expected auto, disable, require, verify-ca or verify-full"))
        }
    }
}

fn parse_ssh_auth(s: &str) -> Result<SshAuth, String> {
    match s {
        "password" => Ok(SshAuth::Password),
        "key-pair" => Ok(SshAuth::KeyPair),
        "agent" => Ok(SshAuth::Agent),
        other => Err(format!("`{other}`: expected password, key-pair or agent")),
    }
}

fn parse_host_key(s: &str) -> Result<HostKeyPolicy, String> {
    match s {
        "strict" => Ok(HostKeyPolicy::Strict),
        "accept-new" => Ok(HostKeyPolicy::AcceptNew),
        other => Err(format!("`{other}`: expected strict or accept-new")),
    }
}

fn parse_property(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().to_owned(), v.trim().to_owned())),
        _ => Err(format!("`{s}`: expected name=value")),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let engine = rowcraft_core::Engine::new(rowcraft_registry::default_registry());
    for warning in rowcraft_config::register_custom_drivers() {
        eprintln!("rowcraft: warning: drivers.toml: {warning}");
    }
    match cli.command {
        Command::Dialects => commands::dialects(&engine),
        Command::Drivers { action: None | Some(DriversCommand::List) } => {
            commands::drivers(&engine)
        }
        Command::Drivers { action: Some(DriversCommand::Add(a)) } => {
            commands::drivers_add(&engine, a)
        }
        Command::Drivers { action: Some(DriversCommand::Rm { id }) } => commands::drivers_rm(&id),
        Command::Ssh(SshCommand::List) => commands::ssh_list(),
        Command::Ssh(SshCommand::Add(a)) => commands::ssh_add(a),
        Command::Ssh(SshCommand::Rm { name }) => commands::ssh_rm(&name),
        Command::Ssh(SshCommand::Test { name }) => commands::ssh_test(&name).await,
        Command::Ds(DsCommand::List) => commands::ds_list(),
        Command::Ds(DsCommand::Add(a)) => commands::ds_add(&engine, *a),
        Command::Ds(DsCommand::Rm { name }) => commands::ds_rm(&name),
        Command::Ds(DsCommand::Test { name }) => commands::ds_test(&engine, &name).await,
        Command::Query(q) => commands::query(&engine, q).await,
        Command::Introspect(a) => commands::introspect(&engine, a).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(args: &[&str]) -> AddArgs {
        let mut argv = vec!["rowcraft", "ds", "add", "x"];
        argv.extend_from_slice(args);
        match Cli::try_parse_from(argv).unwrap().command {
            Command::Ds(DsCommand::Add(a)) => *a,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ds_add_takes_the_new_options() {
        let a = add(&[
            "--driver",
            "mysql",
            "--url",
            "mysql://h/d",
            "--comment",
            "c",
            "--startup-script",
            "set x=1",
            "--schema-pattern",
            "*:s",
            "--object-filter",
            "table:-t",
            "--ssl-mode",
            "verify-ca",
            "--property",
            "a=1",
            "--property",
            "b = two = 2",
        ]);
        assert_eq!(a.driver.as_deref(), Some("mysql"));
        assert_eq!(a.ssl_mode, SslMode::VerifyCa);
        assert_eq!(
            a.properties,
            [("a".to_owned(), "1".to_owned()), ("b".to_owned(), "two = 2".to_owned())]
        );
        assert!(a.dialect.is_none(), "the dialect is derived later, not defaulted by clap");
    }

    #[test]
    fn drivers_subcommands_parse() {
        let bare = Cli::try_parse_from(["rowcraft", "drivers"]).unwrap();
        assert!(matches!(bare.command, Command::Drivers { action: None }));
        let add = Cli::try_parse_from([
            "rowcraft",
            "drivers",
            "add",
            "questdb",
            "--name",
            "QuestDB",
            "--dialect",
            "pg",
            "--port",
            "8812",
        ])
        .unwrap();
        let Command::Drivers { action: Some(DriversCommand::Add(a)) } = add.command else {
            panic!("not drivers add")
        };
        assert_eq!(
            (a.id.as_str(), a.dialect.as_str(), a.port, a.file),
            ("questdb", "pg", Some(8812), false)
        );
        assert!(
            Cli::try_parse_from(["rowcraft", "drivers", "add", "x"]).is_err(),
            "name and dialect are required"
        );
    }

    #[test]
    fn ssh_options_parse() {
        let a = add(&["--ssh", "jump", "--ssh-local-port", "15432"]);
        assert_eq!((a.ssh.as_deref(), a.ssh_local_port), (Some("jump"), Some(15432)));
        let cli = Cli::try_parse_from([
            "rowcraft",
            "ssh",
            "add",
            "jump",
            "--host",
            "h",
            "--user",
            "u",
            "--auth",
            "key-pair",
            "--key-file",
            "~/.ssh/id",
            "--host-key",
            "accept-new",
        ])
        .unwrap();
        let Command::Ssh(SshCommand::Add(s)) = cli.command else { panic!("not ssh add") };
        assert_eq!((s.auth, s.host_key), (SshAuth::KeyPair, HostKeyPolicy::AcceptNew));
        let defaults =
            Cli::try_parse_from(["rowcraft", "ssh", "add", "j", "--host", "h", "--user", "u"])
                .unwrap();
        let Command::Ssh(SshCommand::Add(d)) = defaults.command else { panic!("not ssh add") };
        assert_eq!((d.auth, d.host_key), (SshAuth::Password, HostKeyPolicy::Strict));
        assert!(
            Cli::try_parse_from(["rowcraft", "ssh", "add", "j", "--host", "h"]).is_err(),
            "user is required"
        );
        assert!(
            Cli::try_parse_from([
                "rowcraft", "ssh", "add", "j", "--host", "h", "--user", "u", "--auth", "kerberos"
            ])
            .is_err()
        );
    }

    #[test]
    fn bad_values_are_usage_errors() {
        for bad in [&["--ssl-mode", "maybe"][..], &["--property", "novalue"], &["--property", "=1"]]
        {
            let mut argv = vec!["rowcraft", "ds", "add", "x"];
            argv.extend_from_slice(bad);
            assert!(Cli::try_parse_from(argv).is_err(), "{bad:?}");
        }
    }
}
