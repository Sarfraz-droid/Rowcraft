//! Tunnel tests against an in-process SSH server (russh's own server side), so nothing outside
//! the test process is needed. The server accepts one password and/or one public key, proxies
//! `direct-tcpip` channels to the requested local port, and counts connections.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rowcraft_config::{HostKeyPolicy, SshAuth, SshConfig};
use rowcraft_ssh::{Credentials, Error, RusshOpener, SecretKind, Target, TunnelOpener};
use russh::Channel;
use russh::keys::HashAlg;
use russh::keys::ssh_key::{Algorithm, LineEnding, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PASSWORD: &str = "right-password";
const PASSPHRASE: &str = "s3cret";

fn new_key() -> PrivateKey {
    PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("key")
}

#[derive(Default)]
struct ServerLog {
    connections: AtomicU32,
}

#[derive(Clone)]
struct Handler {
    password: Option<String>,
    client_key: Option<PublicKey>,
}

impl server::Handler for Handler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if self.password.as_deref() == Some(password) { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_publickey(&mut self, _: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(if self.client_key.as_ref() == Some(key) { Auth::Accept } else { Auth::reject() })
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        let Ok(mut upstream) = TcpStream::connect((host, port as u16)).await else {
            return Ok(()); // dropping the handle rejects the channel
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
        });
        Ok(())
    }
}

struct TestServer {
    addr: SocketAddr,
    host_key: PrivateKey,
    log: Arc<ServerLog>,
}

#[derive(Clone)]
struct Factory(Handler);

impl server::Server for Factory {
    type Handler = Handler;
    fn new_client(&mut self, _: Option<SocketAddr>) -> Handler {
        self.0.clone()
    }
}

impl TestServer {
    async fn start(password: Option<&str>, client_key: Option<&PrivateKey>) -> Self {
        Self::start_with(new_key(), password, client_key).await
    }

    async fn start_with(
        host_key: PrivateKey,
        password: Option<&str>,
        client_key: Option<&PrivateKey>,
    ) -> Self {
        let config = server::Config {
            auth_rejection_time: Duration::from_millis(1),
            inactivity_timeout: None,
            keys: vec![host_key.clone()],
            ..Default::default()
        };
        let config = Arc::new(config);
        let handler = Handler {
            password: password.map(str::to_owned),
            client_key: client_key.map(|k| k.public_key().clone()),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log = Arc::new(ServerLog::default());
        let counter = log.clone();
        tokio::spawn(async move {
            let mut factory = Factory(handler);
            while let Ok((socket, peer)) = listener.accept().await {
                counter.connections.fetch_add(1, Ordering::SeqCst);
                let session = factory.new_client(Some(peer));
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(running) = server::run_stream(config, socket, session).await {
                        let _ = running.await;
                    }
                });
            }
        });
        Self { addr, host_key, log }
    }

    fn fingerprint(&self) -> String {
        self.host_key.public_key().fingerprint(HashAlg::Sha256).to_string()
    }

    /// A known_hosts file that trusts this server.
    fn known_hosts(&self, dir: &TempDir) -> PathBuf {
        let line = self.host_key.public_key().to_openssh().expect("openssh");
        let path = dir.path("known_hosts");
        std::fs::write(&path, format!("[127.0.0.1]:{} {line}\n", self.addr.port())).expect("write");
        path
    }

    fn config(&self, dir: &TempDir, trust: bool) -> SshConfig {
        let known_hosts = if trust {
            self.known_hosts(dir)
        } else {
            dir.path("known_hosts") // does not exist
        };
        SshConfig {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port: Some(self.addr.port()),
            user: "tester".into(),
            known_hosts_file: Some(known_hosts.display().to_string()),
            ..Default::default()
        }
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "rowcraft-ssh-{tag}-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        Self(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Answers from a script, one per attempt, and remembers what it was asked.
struct Scripted {
    answers: Vec<Option<&'static str>>,
    asked: Mutex<Vec<(SecretKind, u32, Option<String>)>>,
}

impl Scripted {
    fn new(answers: &[Option<&'static str>]) -> Self {
        Self { answers: answers.to_vec(), asked: Mutex::new(Vec::new()) }
    }
    fn asked(&self) -> Vec<(SecretKind, u32, Option<String>)> {
        self.asked.lock().expect("lock").clone()
    }
}

#[async_trait]
impl Credentials for Scripted {
    async fn secret(
        &self,
        _: &SshConfig,
        kind: SecretKind,
        attempt: u32,
        last: Option<&str>,
    ) -> Option<String> {
        self.asked.lock().expect("lock").push((kind, attempt, last.map(str::to_owned)));
        self.answers.get(attempt as usize).copied().flatten().map(str::to_owned)
    }
}

async fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 || socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

fn target(port: u16) -> Target {
    Target { host: "127.0.0.1".into(), port }
}

async fn open(
    config: &SshConfig,
    creds: &dyn Credentials,
    targets: &[Target],
    forced: Option<u16>,
) -> Result<rowcraft_ssh::Tunnels, Error> {
    RusshOpener.open(config, creds, targets, forced).await
}

async fn round_trip(addr: SocketAddr, message: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect(addr).await.expect("connect to the tunnel");
    socket.write_all(message).await.expect("write");
    let mut reply = vec![0u8; message.len()];
    tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut reply))
        .await
        .expect("echo timed out")
        .expect("read");
    reply
}

#[tokio::test(flavor = "multi_thread")]
async fn bytes_flow_through_the_tunnel_to_the_target() {
    let dir = TempDir::new("flow");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let echo = echo_server().await;
    let tunnels =
        open(&server.config(&dir, true), &Scripted::new(&[Some(PASSWORD)]), &[target(echo)], None)
            .await
            .expect("open");
    assert_eq!(tunnels.endpoints().len(), 1);
    assert!(tunnels.endpoints()[0].ip().is_loopback());
    assert_eq!(
        round_trip(tunnels.endpoints()[0], b"hello through ssh").await,
        b"hello through ssh"
    );
    // A second connection through the same tunnel works too.
    assert_eq!(round_trip(tunnels.endpoints()[0], b"again").await, b"again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_password_is_asked_again_with_the_reason() {
    let dir = TempDir::new("retry");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let creds = Scripted::new(&[Some("nope"), Some(PASSWORD)]);
    open(&server.config(&dir, true), &creds, &[target(1)], None)
        .await
        .expect("second attempt succeeds");
    let asked = creds.asked();
    assert_eq!(asked.len(), 2);
    assert_eq!((asked[0].1, asked[0].2.as_deref()), (0, None));
    assert_eq!(asked[1].1, 1);
    assert!(asked[1].2.as_deref().is_some_and(|r| r.contains("rejected")), "{asked:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn giving_up_after_three_wrong_passwords() {
    let dir = TempDir::new("giveup");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let creds = Scripted::new(&[Some("a"), Some("b"), Some("c"), Some(PASSWORD)]);
    let err = open(&server.config(&dir, true), &creds, &[target(1)], None).await.unwrap_err();
    assert!(matches!(err, Error::AuthFailed(_)), "{err}");
    assert_eq!(creds.asked().len(), 3, "the fourth answer must never be used");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_password_available_says_so_without_trying() {
    let dir = TempDir::new("nocreds");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let mut config = server.config(&dir, true);
    config.password_env = Some("SSH_PW".into());
    let err = open(&config, &Scripted::new(&[None]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(&err, Error::NoCredentials(m) if m.contains("SSH_PW")), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn key_pair_authentication() {
    let dir = TempDir::new("key");
    let client = new_key();
    let server = TestServer::start(None, Some(&client)).await;
    let key_file = dir.path("id");
    std::fs::write(&key_file, client.to_openssh(LineEnding::LF).expect("pem")).expect("write");
    let mut config = server.config(&dir, true);
    config.auth = SshAuth::KeyPair;
    config.key_file = Some(key_file.display().to_string());
    open(&config, &Scripted::new(&[]), &[target(1)], None).await.expect("accepted key");

    // A key the server does not know is rejected, with a clear reason.
    let other = new_key();
    std::fs::write(&key_file, other.to_openssh(LineEnding::LF).expect("pem")).expect("write");
    let err = open(&config, &Scripted::new(&[]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(&err, Error::AuthFailed(m) if m.contains("key")), "{err}");

    config.key_file = Some(dir.path("missing").display().to_string());
    let err = open(&config, &Scripted::new(&[]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(err, Error::KeyFile { .. }), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encrypted_key_asks_for_its_passphrase() {
    let dir = TempDir::new("passphrase");
    let client = new_key();
    let server = TestServer::start(None, Some(&client)).await;
    let key_file = dir.path("id");
    let encrypted = client.encrypt(&mut rand::rng(), PASSPHRASE).expect("encrypt");
    std::fs::write(&key_file, encrypted.to_openssh(LineEnding::LF).expect("pem")).expect("write");
    let mut config = server.config(&dir, true);
    config.auth = SshAuth::KeyPair;
    config.key_file = Some(key_file.display().to_string());
    config.passphrase_env = Some("KEY_PW".into());

    let creds = Scripted::new(&[Some("wrong"), Some(PASSPHRASE)]);
    open(&config, &creds, &[target(1)], None).await.expect("second passphrase works");
    let asked = creds.asked();
    assert_eq!(asked.iter().map(|a| a.0).collect::<Vec<_>>(), [SecretKind::KeyPassphrase; 2]);
    assert!(asked[1].2.as_deref().is_some_and(|r| r.contains("wrong passphrase")), "{asked:?}");

    let err = open(&config, &Scripted::new(&[None]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(&err, Error::NoCredentials(m) if m.contains("KEY_PW")), "{err}");

    let creds = Scripted::new(&[Some("x"), Some("y"), Some("z")]);
    let err = open(&config, &creds, &[target(1)], None).await.unwrap_err();
    assert!(matches!(err, Error::KeyFile { .. }), "{err}");
    assert_eq!(creds.asked().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_host_is_refused_by_default_and_the_fingerprint_is_shown() {
    let dir = TempDir::new("strict");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let config = server.config(&dir, false);
    assert_eq!(config.host_key, HostKeyPolicy::Strict);
    let err =
        open(&config, &Scripted::new(&[Some(PASSWORD)]), &[target(1)], None).await.unwrap_err();
    match err {
        Error::HostKeyUnknown { fingerprint, host, .. } => {
            assert_eq!(fingerprint, server.fingerprint());
            assert_eq!(host, "127.0.0.1");
        }
        other => panic!("{other}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accept_new_remembers_the_host_and_a_changed_key_is_still_refused() {
    let dir = TempDir::new("acceptnew");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let mut config = server.config(&dir, false);
    config.host_key = HostKeyPolicy::AcceptNew;
    open(&config, &Scripted::new(&[Some(PASSWORD)]), &[target(1)], None)
        .await
        .expect("first contact trusted");

    let learned = std::fs::read_to_string(config.known_hosts_file.as_deref().expect("file"))
        .expect("known_hosts written");
    assert!(learned.contains(&format!("[127.0.0.1]:{}", server.addr.port())), "{learned}");

    // Now strict is satisfied by what was learned...
    config.host_key = HostKeyPolicy::Strict;
    open(&config, &Scripted::new(&[Some(PASSWORD)]), &[target(1)], None).await.expect("remembered");

    // ...and a different server key on the same address is refused even with accept-new.
    let imposter = TestServer::start_with(new_key(), Some(PASSWORD), None).await;
    let mut config = imposter.config(&dir, false);
    config.known_hosts_file = Some(dir.path("pinned").display().to_string());
    std::fs::write(
        dir.path("pinned"),
        format!(
            "[127.0.0.1]:{} {}\n",
            imposter.addr.port(),
            new_key().public_key().to_openssh().expect("openssh")
        ),
    )
    .expect("write");
    config.host_key = HostKeyPolicy::AcceptNew;
    let err =
        open(&config, &Scripted::new(&[Some(PASSWORD)]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(err, Error::HostKeyChanged { line: 1, .. }), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forced_port_that_is_taken_fails_before_any_ssh_login() {
    let dir = TempDir::new("occupied");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let taken = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = taken.local_addr().expect("addr").port();
    let err = open(
        &server.config(&dir, true),
        &Scripted::new(&[Some(PASSWORD)]),
        &[target(1)],
        Some(port),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::ForcedPortOccupied(p) if p == port), "{err}");
    assert_eq!(
        server.log.connections.load(Ordering::SeqCst),
        0,
        "no SSH connection should have been attempted"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forced_port_is_used_and_cannot_serve_several_tunnels() {
    let dir = TempDir::new("forced");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let echo = echo_server().await;
    let free =
        TcpListener::bind("127.0.0.1:0").await.expect("bind").local_addr().expect("addr").port();
    let tunnels = open(
        &server.config(&dir, true),
        &Scripted::new(&[Some(PASSWORD)]),
        &[target(echo)],
        Some(free),
    )
    .await
    .expect("open");
    assert_eq!(tunnels.endpoints()[0].port(), free);
    assert_eq!(round_trip(tunnels.endpoints()[0], b"x").await, b"x");
    drop(tunnels);

    let err = open(
        &server.config(&dir, true),
        &Scripted::new(&[Some(PASSWORD)]),
        &[target(echo), target(echo)],
        Some(free),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::ForcedPortWithMultipleTunnels(2, p) if p == free), "{err}");
    assert_eq!(
        server.log.connections.load(Ordering::SeqCst),
        1,
        "only the first open should have connected"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cluster_gets_one_tunnel_per_node() {
    let dir = TempDir::new("cluster");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let (a, b) = (echo_server().await, echo_server().await);
    let tunnels = open(
        &server.config(&dir, true),
        &Scripted::new(&[Some(PASSWORD)]),
        &[target(a), target(b)],
        None,
    )
    .await
    .expect("open");
    let ports: Vec<_> = tunnels.endpoints().iter().map(|e| e.port()).collect();
    assert_eq!(ports.len(), 2);
    assert_ne!(ports[0], ports[1]);
    assert_eq!(round_trip(tunnels.endpoints()[0], b"node a").await, b"node a");
    assert_eq!(round_trip(tunnels.endpoints()[1], b"node b").await, b"node b");
    assert_eq!(
        server.log.connections.load(Ordering::SeqCst),
        1,
        "both tunnels share one SSH session"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_tunnels_closes_the_local_ports() {
    let dir = TempDir::new("drop");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let echo = echo_server().await;
    let tunnels =
        open(&server.config(&dir, true), &Scripted::new(&[Some(PASSWORD)]), &[target(echo)], None)
            .await
            .expect("open");
    let addr = tunnels.endpoints()[0];
    assert!(TcpStream::connect(addr).await.is_ok());
    drop(tunnels);
    let mut closed = false;
    for _ in 0..50 {
        if TcpStream::connect(addr).await.is_err() {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(closed, "the listener should be gone after the tunnels are dropped");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_configuration_that_cannot_work_is_refused_up_front() {
    let dir = TempDir::new("invalid");
    let server = TestServer::start(Some(PASSWORD), None).await;
    let mut config = server.config(&dir, true);
    config.user = String::new();
    let err = open(&config, &Scripted::new(&[]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(&err, Error::Config(m) if m.contains("user")), "{err}");
    let err = open(&server.config(&dir, true), &Scripted::new(&[]), &[], None).await.unwrap_err();
    assert!(matches!(err, Error::Target(_)), "{err}");
    assert_eq!(server.log.connections.load(Ordering::SeqCst), 0);

    // An unreachable SSH host is a connect error naming where it tried.
    let mut dead = server.config(&dir, true);
    dead.port = Some(
        TcpListener::bind("127.0.0.1:0").await.expect("bind").local_addr().expect("addr").port(),
    );
    let err = open(&dead, &Scripted::new(&[]), &[target(1)], None).await.unwrap_err();
    assert!(matches!(&err, Error::Connect { host, .. } if host == "127.0.0.1"), "{err}");
}
