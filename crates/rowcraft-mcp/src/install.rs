//! Registering the server with local AI clients by running their own `mcp add` commands.
//!
//! Shared by the CLI (`rowcraft mcp install`) and the desktop app. A desktop app started from
//! the Dock or Finder has a minimal `PATH`, so clients are also looked for in the places they
//! usually install to.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Name the server is registered under.
pub const NAME: &str = "rowcraft";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    Claude,
    Codex,
}

impl Client {
    pub const ALL: [Client; 2] = [Client::Claude, Client::Codex];

    pub fn id(self) -> &'static str {
        match self {
            Client::Claude => "claude",
            Client::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Client::Claude => "Claude Code",
            Client::Codex => "Codex",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// `PATH` plus the usual install locations, in search order.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        for rel in [".local/bin", ".claude/local", ".npm-global/bin", ".cargo/bin", ".bun/bin"] {
            dirs.push(home.join(rel));
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    for fixed in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(fixed));
    }
    dirs
}

/// The executable for `client`, if it is installed.
pub fn find(client: Client) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    search_dirs().into_iter().find_map(|dir| {
        names.iter().map(|ext| dir.join(format!("{}{ext}", client.id()))).find(|p| p.is_file())
    })
}

/// What a client runs to start the server: `exe mcp [--allow-write]`.
pub fn server_command(exe: &Path, allow_write: bool) -> Vec<String> {
    let mut command = vec![exe.display().to_string(), "mcp".to_owned()];
    if allow_write {
        command.push("--allow-write".to_owned());
    }
    command
}

/// Arguments for `<client> mcp add`. `scope` only applies to Claude Code.
pub fn add_args(client: Client, scope: &str, command: &[String]) -> Vec<String> {
    let mut args: Vec<String> = match client {
        Client::Claude => ["mcp", "add", "--scope", scope, NAME, "--"].map(str::to_owned).to_vec(),
        Client::Codex => ["mcp", "add", NAME, "--"].map(str::to_owned).to_vec(),
    };
    args.extend(command.iter().cloned());
    args
}

fn remove_args(client: Client, scope: &str) -> Vec<String> {
    match client {
        Client::Claude => ["mcp", "remove", NAME, "--scope", scope].map(str::to_owned).to_vec(),
        Client::Codex => ["mcp", "remove", NAME].map(str::to_owned).to_vec(),
    }
}

/// Runs `client` with `args`; the error is what the client printed.
fn run(client: Client, args: &[String]) -> Result<(), String> {
    let binary = find(client).ok_or_else(|| format!("`{}` was not found", client.id()))?;
    let path = std::env::join_paths(search_dirs()).unwrap_or_default();
    let output = Command::new(&binary)
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", binary.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let text = if text.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        text
    };
    Err(if text.is_empty() {
        format!("{} exited with {}", client.id(), output.status)
    } else {
        text
    })
}

pub fn is_registered(client: Client) -> bool {
    run(client, &["mcp".to_owned(), "get".to_owned(), NAME.to_owned()]).is_ok()
}

/// Registers `command` as the `rowcraft` server, replacing an earlier registration.
pub fn register(client: Client, scope: &str, command: &[String]) -> Result<(), String> {
    if is_registered(client) {
        unregister(client, scope)?;
    }
    run(client, &add_args(client, scope, command))
}

pub fn unregister(client: Client, scope: &str) -> Result<(), String> {
    run(client, &remove_args(client, scope))
}

/// The `mcpServers` JSON that other clients (Cursor, Claude Desktop, …) take.
pub fn config_json(command: &[String]) -> String {
    let config = serde_json::json!({
        "mcpServers": { NAME: { "command": command[0], "args": command[1..] } }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn commands_match_each_clients_cli() {
        let cmd = server_command(Path::new("/app/rowcraft"), true);
        assert_eq!(cmd, ["/app/rowcraft", "mcp", "--allow-write"]);
        assert_eq!(
            add_args(Client::Claude, "user", &cmd),
            [
                "mcp",
                "add",
                "--scope",
                "user",
                "rowcraft",
                "--",
                "/app/rowcraft",
                "mcp",
                "--allow-write"
            ]
        );
        assert_eq!(
            add_args(Client::Codex, "user", &cmd[..2]),
            ["mcp", "add", "rowcraft", "--", "/app/rowcraft", "mcp"]
        );
    }

    #[test]
    fn clients_parse_by_id() {
        assert_eq!(Client::parse("codex"), Some(Client::Codex));
        assert_eq!(Client::parse("vim"), None);
    }

    #[test]
    fn config_json_names_the_command_and_args() {
        let json: serde_json::Value =
            serde_json::from_str(&config_json(&server_command(Path::new("/x/rowcraft"), false)))
                .unwrap();
        assert_eq!(json["mcpServers"]["rowcraft"]["command"], "/x/rowcraft");
        assert_eq!(json["mcpServers"]["rowcraft"]["args"][0], "mcp");
    }
}
