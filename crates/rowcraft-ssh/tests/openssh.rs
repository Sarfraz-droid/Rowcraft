//! Interoperability with a real OpenSSH server. Skipped unless `ROWCRAFT_TEST_SSH_PORT` is set;
//! `make test-ssh` starts a throwaway `sshd` and sets everything:
//!
//! - `ROWCRAFT_TEST_SSH_PORT`: the port `sshd` listens on, on 127.0.0.1
//! - `ROWCRAFT_TEST_SSH_USER`: the user to log in as
//! - `ROWCRAFT_TEST_SSH_DIR`: holds `client`, `client_enc` (passphrase `s3cret`) and `known_hosts`

use std::path::PathBuf;

use async_trait::async_trait;
use rowcraft_config::{SshAuth, SshConfig};
use rowcraft_ssh::{Credentials, Error, RusshOpener, SecretKind, Target, TunnelOpener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

struct Env {
    port: u16,
    user: String,
    dir: PathBuf,
}

fn env() -> Option<Env> {
    Some(Env {
        port: std::env::var("ROWCRAFT_TEST_SSH_PORT").ok()?.parse().ok()?,
        user: std::env::var("ROWCRAFT_TEST_SSH_USER").ok()?,
        dir: std::env::var("ROWCRAFT_TEST_SSH_DIR").ok()?.into(),
    })
}

macro_rules! live {
    () => {
        match env() {
            Some(e) => e,
            None => {
                eprintln!("skipped: ROWCRAFT_TEST_SSH_PORT not set");
                return;
            }
        }
    };
}

fn config(e: &Env, key: &str, known_hosts: &str) -> SshConfig {
    SshConfig {
        name: "openssh".into(),
        host: "127.0.0.1".into(),
        port: Some(e.port),
        user: e.user.clone(),
        auth: SshAuth::KeyPair,
        key_file: Some(e.dir.join(key).display().to_string()),
        known_hosts_file: Some(e.dir.join(known_hosts).display().to_string()),
        ..Default::default()
    }
}

struct Passphrase(&'static str);

#[async_trait]
impl Credentials for Passphrase {
    async fn secret(
        &self,
        _: &SshConfig,
        kind: SecretKind,
        attempt: u32,
        _: Option<&str>,
    ) -> Option<String> {
        (kind == SecretKind::KeyPassphrase && attempt == 0).then(|| self.0.to_owned())
    }
}

async fn echo() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 256];
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

#[tokio::test(flavor = "multi_thread")]
async fn bytes_flow_through_a_real_sshd() {
    let e = live!();
    let port = echo().await;
    let target = Target { host: "127.0.0.1".into(), port };
    let tunnels = RusshOpener
        .open(&config(&e, "client", "known_hosts"), &Passphrase("unused"), &[target], None)
        .await
        .expect("open through OpenSSH");
    let mut socket = TcpStream::connect(tunnels.endpoints()[0]).await.expect("connect");
    socket.write_all(b"via openssh").await.expect("write");
    let mut reply = [0u8; 11];
    socket.read_exact(&mut reply).await.expect("read");
    assert_eq!(&reply, b"via openssh");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encrypted_key_works_with_its_passphrase_and_not_without() {
    let e = live!();
    let target = Target { host: "127.0.0.1".into(), port: 1 };
    let ok = RusshOpener
        .open(
            &config(&e, "client_enc", "known_hosts"),
            &Passphrase("s3cret"),
            std::slice::from_ref(&target),
            None,
        )
        .await;
    assert!(ok.is_ok(), "{:?}", ok.err());
    let wrong = RusshOpener
        .open(&config(&e, "client_enc", "known_hosts"), &Passphrase("wrong"), &[target], None)
        .await
        .unwrap_err();
    assert!(matches!(wrong, Error::KeyFile { .. }), "{wrong}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_missing_from_known_hosts_is_refused() {
    let e = live!();
    let target = Target { host: "127.0.0.1".into(), port: 1 };
    let err = RusshOpener
        .open(&config(&e, "client", "no_such_known_hosts"), &Passphrase("x"), &[target], None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::HostKeyUnknown { .. }), "{err}");
}
