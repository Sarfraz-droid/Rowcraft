//! Manage the app-owned router and explicitly stop verified local external routers.
//! Automatic cleanup only stops the process launched by this app.

use serde::Serialize;
use std::{
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const ROUTER_VERSION: &str = "0.5.104";

#[derive(Debug, Serialize)]
pub struct RouterStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<&'static str>,
    pub managed: bool,
    pub pid: Option<u32>,
    pub base_url: Option<String>,
}

#[derive(Debug, Default)]
struct Running {
    child: Option<Child>,
    port: Option<u16>,
}

#[derive(Debug)]
pub struct RouterManager {
    directory: PathBuf,
    running: Mutex<Running>,
    cancelling: AtomicBool,
}

impl RouterManager {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            running: Mutex::new(Running::default()),
            cancelling: AtomicBool::new(false),
        }
    }

    fn server(&self) -> PathBuf {
        self.directory.join("node_modules/9router/app/custom-server.js")
    }

    fn installed(&self) -> bool {
        self.server().exists()
            && self.directory.join("node_modules/sql.js/dist/sql-wasm.wasm").exists()
            && std::fs::read_to_string(self.directory.join("installed-version"))
                .is_ok_and(|v| v == ROUTER_VERSION)
    }

    pub fn status(&self) -> Result<RouterStatus, String> {
        let mut running =
            self.running.lock().map_err(|_| "Router state is unavailable.".to_owned())?;
        if let Some(child) = &mut running.child {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                running.child = None;
                running.port = None;
            }
        }
        Ok(RouterStatus {
            installed: self.installed(),
            running: running.child.is_some(),
            version: Some(ROUTER_VERSION),
            managed: running.child.is_some(),
            pid: running.child.as_ref().map(Child::id),
            base_url: running.port.map(|port| format!("http://127.0.0.1:{port}/v1")),
        })
    }

    pub fn start(&self, base_url: &str) -> Result<RouterStatus, String> {
        self.cancelling.store(false, Ordering::SeqCst);
        let settings = rowcraft_ui_model::ai::normalize(&rowcraft_ui_model::ai::AiSettings {
            base_url: base_url.into(),
            ..Default::default()
        })?;
        let url = reqwest::Url::parse(&settings.base_url).map_err(|e| e.to_string())?;
        if url.scheme() != "http"
            || ![Some("localhost"), Some("127.0.0.1")].contains(&url.host_str())
            || url.path() != "/v1"
        {
            return Err("The managed router uses http://127.0.0.1:<port>/v1. Remote gateways can be connected without starting a local router.".into());
        }
        let port = url.port_or_known_default().ok_or("Choose a port for the local router.")?;
        let current = self.status()?;
        if current.running {
            if current.base_url.as_deref() == Some(&format!("http://127.0.0.1:{port}/v1")) {
                return Ok(current);
            }
            return Err("Stop the current router before changing its port.".into());
        }
        let mut running =
            self.running.lock().map_err(|_| "Router state is unavailable.".to_owned())?;
        if running.child.is_some() {
            return Err("The router is already starting.".into());
        }
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
            return Err(format!(
                "Port {port} is already in use. Connect to that router or choose another port; use Stop in AI settings to close an existing local 9router."
            ));
        }
        let node = find_node()?;
        std::fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let log = || {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.directory.join("router.log"))
                .map_err(|e| e.to_string())
        };
        if !self.installed() {
            let npm = node
                .parent()
                .ok_or("Node installation is invalid.")?
                .join("node_modules/npm/bin/npm-cli.js");
            let npm = if npm.exists() {
                npm
            } else {
                node.parent().ok_or("Node installation is invalid.")?.join("npm")
            };
            if !npm.exists() {
                return Err(
                    "Install Node.js 22 or newer with npm to start the managed router.".into()
                );
            }
            std::fs::write(
                self.directory.join("package.json"),
                "{\"name\":\"rowcraft-ai-router\",\"private\":true}",
            )
            .map_err(|e| e.to_string())?;
            let mut command = Command::new(&node);
            command
                .arg(npm)
                .args([
                    "install",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                    "--omit=dev",
                    &format!("9router@{ROUTER_VERSION}"),
                    "sql.js@1.14.1",
                ])
                .current_dir(&self.directory)
                .env("PATH", node_path(&node)?)
                .stdin(Stdio::null())
                .stdout(log()?)
                .stderr(log()?);
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            let mut installer =
                command.spawn().map_err(|e| format!("Could not install 9router: {e}"))?;
            let deadline = Instant::now() + Duration::from_secs(180);
            loop {
                if self.cancelling.load(Ordering::SeqCst) {
                    terminate(&mut installer);
                    return Err("Router installation cancelled.".into());
                }
                if let Some(status) = installer.try_wait().map_err(|e| e.to_string())? {
                    if !status.success() {
                        return Err(format!(
                            "9router installation failed. Check {}",
                            self.directory.join("router.log").display()
                        ));
                    }
                    // npm omits WASM files from nested bundled dependencies. Supply the
                    // packaged sql.js resolver with the explicitly installed runtime WASM.
                    let wasm = self.directory.join("node_modules/sql.js/dist/sql-wasm.wasm");
                    let bundled = self
                        .directory
                        .join("node_modules/9router/app/node_modules/sql.js/dist/sql-wasm.wasm");
                    if let Some(parent) = bundled.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::copy(wasm, bundled).map_err(|e| {
                        format!("Could not prepare the router database runtime: {e}")
                    })?;
                    std::fs::write(self.directory.join("installed-version"), ROUTER_VERSION)
                        .map_err(|e| e.to_string())?;
                    break;
                }
                if Instant::now() > deadline {
                    terminate(&mut installer);
                    return Err(
                        "9router installation timed out. Check the network and try Start again."
                            .into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let server = self.server();
        let app_dir = server.parent().ok_or("9router installation is incomplete.")?;
        let mut command = Command::new(&node);
        command
            .arg("--dns-result-order=ipv4first")
            .arg(&server)
            .current_dir(app_dir)
            .env("HOSTNAME", "127.0.0.1")
            .env("PORT", port.to_string())
            .env("DATA_DIR", self.directory.join("data"))
            .env("NEXT_PUBLIC_BASE_URL", format!("http://127.0.0.1:{port}"))
            .env("NODE_PATH", self.directory.join("node_modules"))
            .env("PATH", node_path(&node)?)
            .stdin(Stdio::null())
            .stdout(log()?)
            .stderr(log()?);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| format!("Could not start 9router: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if self.cancelling.load(Ordering::SeqCst) {
                terminate(&mut child);
                return Err("Router startup cancelled.".into());
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!(
                    "9router exited ({status}). Check {}",
                    self.directory.join("router.log").display()
                ));
            }
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                break;
            }
            if Instant::now() > deadline {
                terminate(&mut child);
                return Err("9router did not become ready within 45 seconds. Check router.log and try again.".into());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        running.child = Some(child);
        running.port = Some(port);
        drop(running);
        self.status()
    }

    pub fn external_pid(base_url: &str) -> Result<Option<u32>, String> {
        let url = reqwest::Url::parse(base_url).map_err(|e| e.to_string())?;
        if ![Some("localhost"), Some("127.0.0.1"), Some("[::1]")].contains(&url.host_str()) {
            return Ok(None);
        }
        let port = url.port_or_known_default().ok_or("Choose a local router port.")?;
        Ok(local_listeners(port)?
            .into_iter()
            .find_map(|(pid, identity)| is_router_identity(&identity).then_some(pid)))
    }

    pub fn stop_external(&self, base_url: &str) -> Result<RouterStatus, String> {
        let pid = Self::external_pid(base_url)?.ok_or(
            "No local 9router process was found at this gateway. Refresh its status and try again.",
        )?;
        // Check again immediately before signaling; never signal a process group
        // belonging to an external launcher or a service on another port.
        if Self::external_pid(base_url)? != Some(pid) {
            return Err("The router process changed. Refresh its status and try again.".into());
        }
        #[cfg(unix)]
        let result = Command::new("/bin/kill").args(["-TERM", &pid.to_string()]).output();
        #[cfg(windows)]
        let result = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output();
        let result = result.map_err(|e| format!("Could not stop 9router: {e}"))?;
        if !result.status.success() {
            return Err(format!(
                "Could not stop 9router: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Self::external_pid(base_url)?.is_some() {
            if Instant::now() >= deadline {
                return Err("9router is still running or was restarted by another program. Stop its launcher and try again.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.status()
    }

    pub fn stop(&self) -> Result<RouterStatus, String> {
        self.cancelling.store(true, Ordering::SeqCst);
        let mut running =
            self.running.lock().map_err(|_| "Router state is unavailable.".to_owned())?;
        if let Some(mut child) = running.child.take() {
            terminate(&mut child);
        }
        running.port = None;
        drop(running);
        self.status()
    }
}

impl Drop for RouterManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn is_router_identity(identity: &str) -> bool {
    identity
        .split(|c: char| c.is_whitespace() || matches!(c, '/' | '\\' | '\'' | '"'))
        .any(|part| part == "9router" || part.starts_with("9router@"))
}

#[cfg(unix)]
fn local_listeners(port: u16) -> Result<Vec<(u32, String)>, String> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
        .output()
        .map_err(|e| format!("Could not inspect the local router: {e}"))?;
    let mut listeners = Vec::new();
    for pid in
        String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| line.parse::<u32>().ok())
    {
        let command = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .map_err(|e| e.to_string())?;
        // Next.js may replace the process title. Its working directory still
        // identifies the installed package even when the command is next-server.
        let cwd = Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .map_err(|e| e.to_string())?;
        listeners.push((
            pid,
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&command.stdout),
                String::from_utf8_lossy(&cwd.stdout)
            ),
        ));
    }
    Ok(listeners)
}

#[cfg(windows)]
fn local_listeners(port: u16) -> Result<Vec<(u32, String)>, String> {
    let script = format!(
        "Get-NetTCPConnection -State Listen -LocalPort {port} -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique | ForEach-Object {{ $process = Get-CimInstance Win32_Process -Filter \"ProcessId = $_\"; Write-Output \"$($_)`t$($process.CommandLine)\" }}"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("Could not inspect the local router: {e}"))?;
    if !output.status.success() {
        return Err("Could not inspect the local router processes.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.split_once('\t')?;
            Some((pid.parse().ok()?, command.to_owned()))
        })
        .collect())
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill").args(["-TERM", &format!("-{}", child.id())]).status();
    }
    #[cfg(windows)]
    {
        let _ =
            Command::new("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()]).status();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill").args(["-KILL", &format!("-{}", child.id())]).status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn node_path(node: &Path) -> Result<std::ffi::OsString, String> {
    let mut paths = vec![node.parent().ok_or("Node installation is invalid.")?.to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    std::env::join_paths(paths).map_err(|e| e.to_string())
}

fn find_node() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(node) = std::env::var_os("ROWCRAFT_NODE") {
        candidates.push(PathBuf::from(node));
    }
    candidates.extend(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(if cfg!(windows) { "node.exe" } else { "node" })),
    );
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut versions: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            versions.sort_by_key(|p| {
                p.file_name().map(|name| {
                    name.to_string_lossy()
                        .trim_start_matches('v')
                        .split('.')
                        .map(|v| v.parse::<u32>().unwrap_or(0))
                        .collect::<Vec<_>>()
                })
            });
            candidates.extend(versions.into_iter().rev().map(|dir| dir.join("bin/node")));
        }
        candidates.push(home.join(".volta/bin/node"));
    }
    candidates
        .extend([PathBuf::from("/opt/homebrew/bin/node"), PathBuf::from("/usr/local/bin/node")]);
    candidates.into_iter().find(|path| path.is_file() && Command::new(path).arg("--version").output().is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim().trim_start_matches('v').split('.').next().and_then(|s| s.parse::<u32>().ok()).is_some_and(|major| major >= 22)))
        .ok_or_else(|| "Install Node.js 22 or newer with npm, then restart the app. You can also set ROWCRAFT_NODE to its executable path.".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    #[test]
    fn stop_only_terminates_the_process_group_owned_by_the_app() {
        let mut owned = Command::new("/bin/sleep");
        owned.arg("60").process_group(0);
        let child = owned.spawn().expect("owned process");
        let mut other = Command::new("/bin/sleep").arg("60").spawn().expect("unrelated process");
        let manager =
            RouterManager::new(std::env::temp_dir().join("rowcraft-router-lifecycle-test"));
        {
            let mut running = manager.running.lock().expect("state");
            running.child = Some(child);
            running.port = Some(20128);
        }
        assert!(manager.status().expect("status").running);
        assert!(!manager.stop().expect("stop").running);
        let unrelated_is_running = other.try_wait().expect("other status").is_none();
        let _ = other.kill();
        let _ = other.wait();
        assert!(unrelated_is_running);
    }

    #[test]
    fn identifies_router_package_paths_without_matching_other_names() {
        assert!(is_router_identity(
            "node /usr/local/lib/node_modules/9router/app/custom-server.js"
        ));
        assert!(is_router_identity(
            "next-server\nn/Users/me/.npm/_npx/install/node_modules/9router/app"
        ));
        assert!(is_router_identity(r"node C:\npm\node_modules\9router\app\custom-server.js"));
        assert!(!is_router_identity("node /tmp/not-9router/server.js"));
        assert!(!is_router_identity("node /tmp/9router-backup/server.js"));
    }

    #[test]
    fn external_detection_does_not_inspect_remote_gateway_processes() {
        assert_eq!(RouterManager::external_pid("https://example.com/v1").expect("remote"), None);
    }

    #[test]
    #[ignore = "subprocess fixture for external router lifecycle tests"]
    fn external_router_fixture() {
        let port = std::env::var("ROWCRAFT_ROUTER_TEST_PORT").expect("fixture port");
        let _listener = std::net::TcpListener::bind(format!("127.0.0.1:{port}")).expect("listen");
        std::thread::sleep(Duration::from_secs(60));
    }

    fn external_fixture(router: bool) -> (Child, PathBuf, String) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
        let port = listener.local_addr().expect("address").port();
        let directory =
            std::env::temp_dir().join(format!("rowcraft-external-{}-{port}", std::process::id()));
        let cwd =
            if router { directory.join("node_modules/9router/app") } else { directory.clone() };
        std::fs::create_dir_all(&cwd).expect("fixture directory");
        drop(listener);
        let child = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "ai_router::tests::external_router_fixture", "--ignored"])
            .env("ROWCRAFT_ROUTER_TEST_PORT", port.to_string())
            .current_dir(cwd)
            .stdout(Stdio::null())
            .spawn()
            .expect("fixture process");
        let deadline = Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "fixture did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        (child, directory, format!("http://127.0.0.1:{port}/v1"))
    }

    #[test]
    fn explicit_stop_closes_an_external_router_but_drop_leaves_it_running() {
        let (mut child, directory, url) = external_fixture(true);
        let manager = RouterManager::new(directory.join("managed"));
        let detected = RouterManager::external_pid(&url).expect("detect router");
        drop(manager);
        let survived_drop = child.try_wait().expect("status").is_none();
        let manager = RouterManager::new(directory.join("managed"));
        let stopped = manager.stop_external(&url);
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(directory);
        assert_eq!(detected, Some(child.id()));
        assert!(survived_drop);
        assert!(!stopped.expect("stop external router").running);
    }

    #[test]
    fn external_stop_rejects_other_services_and_remote_gateways() {
        let (mut child, directory, url) = external_fixture(false);
        let manager = RouterManager::new(directory.join("managed"));
        let detected = RouterManager::external_pid(&url).expect("detect service");
        let stopped = manager.stop_external(&url);
        let still_running = child.try_wait().expect("status").is_none();
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(directory);
        assert_eq!(detected, None);
        assert!(stopped.is_err());
        assert!(still_running);
        assert_eq!(RouterManager::external_pid("https://example.com/v1").expect("remote"), None);
    }

    #[test]
    fn managed_start_rejects_remote_gateways_before_launching_a_process() {
        let manager = RouterManager::new(std::env::temp_dir().join("rowcraft-router-remote-test"));
        assert!(
            manager
                .start("https://example.com/v1")
                .expect_err("remote endpoint")
                .contains("managed router")
        );
        assert!(!manager.status().expect("status").running);
    }
}
