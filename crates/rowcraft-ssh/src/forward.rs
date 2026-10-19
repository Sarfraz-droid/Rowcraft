//! Local listeners that forward each connection through the SSH session.

use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;

use rowcraft_config::SshConfig;
use russh::client::Handle;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::session::{self, Client};
use crate::{Credentials, Error, Result, Target};

/// Open tunnels. Dropping the set closes the listeners and the SSH session.
pub struct Tunnels {
    endpoints: Vec<SocketAddr>,
    tasks: Vec<JoinHandle<()>>,
    _guard: Box<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for Tunnels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tunnels").field("endpoints", &self.endpoints).finish_non_exhaustive()
    }
}

impl Tunnels {
    /// Local address for each target, in the order the targets were given.
    pub fn endpoints(&self) -> &[SocketAddr] {
        &self.endpoints
    }
}

impl Drop for Tunnels {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub(crate) fn from_parts(endpoints: Vec<SocketAddr>, guard: Box<dyn Any + Send + Sync>) -> Tunnels {
    Tunnels { endpoints, tasks: Vec::new(), _guard: guard }
}

pub(crate) async fn open(
    config: &SshConfig,
    credentials: &dyn Credentials,
    targets: &[Target],
    forced_local_port: Option<u16>,
) -> Result<Tunnels> {
    config
        .validate()
        .map_err(|m| Error::Config(m.trim_start_matches("SSH configuration: ").to_owned()))?;
    if targets.is_empty() {
        return Err(Error::Target("nothing to tunnel to".into()));
    }
    if let Some(port) = forced_local_port.filter(|_| targets.len() > 1) {
        return Err(Error::ForcedPortWithMultipleTunnels(targets.len(), port));
    }

    // Bind first: a busy forced port should fail before an SSH login is attempted.
    let mut listeners = Vec::with_capacity(targets.len());
    for _ in targets {
        let wanted = forced_local_port.unwrap_or(0);
        let listener = TcpListener::bind(("127.0.0.1", wanted)).await.map_err(|e| {
            if forced_local_port.is_some() && e.kind() == std::io::ErrorKind::AddrInUse {
                Error::ForcedPortOccupied(wanted)
            } else {
                Error::Io(e)
            }
        })?;
        listeners.push(listener);
    }

    let handle = Arc::new(session::connect(config, credentials).await?);
    let mut endpoints = Vec::with_capacity(targets.len());
    let mut tasks = Vec::with_capacity(targets.len());
    for (listener, target) in listeners.into_iter().zip(targets) {
        endpoints.push(listener.local_addr()?);
        tasks.push(tokio::spawn(accept_loop(listener, handle.clone(), target.clone())));
    }
    Ok(Tunnels { endpoints, tasks, _guard: Box::new(handle) })
}

async fn accept_loop(listener: TcpListener, handle: Arc<Handle<Client>>, target: Target) {
    while let Ok((socket, peer)) = listener.accept().await {
        let (handle, target) = (handle.clone(), target.clone());
        tokio::spawn(async move {
            let _ = pipe(socket, peer, &handle, &target).await;
        });
    }
}

async fn pipe(
    mut socket: TcpStream,
    peer: SocketAddr,
    handle: &Handle<Client>,
    target: &Target,
) -> std::io::Result<()> {
    let channel = handle
        .channel_open_direct_tcpip(
            target.host.clone(),
            u32::from(target.port),
            peer.ip().to_string(),
            u32::from(peer.port()),
        )
        .await
        .map_err(std::io::Error::other)?;
    let mut stream = channel.into_stream();
    tokio::io::copy_bidirectional(&mut socket, &mut stream).await.map(|_| ())
}
