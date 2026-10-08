//! Unix socket server for the UI (docs/ipc.md).
//!
//! Nothing here can block the TX thread: the engine only `try_send`s to the broadcaster and
//! `try_recv`s requests. A client whose queue fills up is dropped, never waited for.

use crate::engine::ClientRequest;
use crate::protocol::{self, ack, AlarmEvent, Out, Request};
use anyhow::{Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Messages queued per client before it is dropped as too slow
const CLIENT_QUEUE: usize = 64;

type Queue = Sender<Arc<String>>;

struct Client {
    id: u64,
    queue: Queue,
    /// Kept to cut the connection when the client is dropped, so the UI sees the close and
    /// reconnects instead of silently receiving nothing
    stream: UnixStream,
}

/// State a new client needs, kept by the broadcaster
#[derive(Default)]
struct Hub {
    clients: Vec<Client>,
    alarms: BTreeMap<&'static str, Arc<String>>,
    params: Option<Arc<String>>,
}

impl Hub {
    /// hello, active alarms and the parameter tree: what `get_state` and a new connection get
    fn send_state(&self, queue: &Queue) {
        let _ = queue.try_send(Arc::new(protocol::hello()));
        for alarm in self.alarms.values() {
            let _ = queue.try_send(alarm.clone());
        }
        if let Some(params) = &self.params {
            let _ = queue.try_send(params.clone());
        }
    }

    fn broadcast(&mut self, message: Arc<String>) {
        self.clients.retain(|client| match client.queue.try_send(message.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                log::warn!("ipc client {} too slow: dropped", client.id);
                let _ = client.stream.shutdown(std::net::Shutdown::Both);
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        });
    }
}

pub struct IpcServer {
    pub threads: Vec<JoinHandle<()>>,
}

pub fn start(
    socket_path: &Path,
    out: Receiver<Out>,
    requests: Sender<ClientRequest>,
    shutdown: Arc<AtomicBool>,
) -> Result<IpcServer> {
    if let Some(dir) = socket_path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    // A stale socket from a crashed run would make bind fail
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("bind {}", socket_path.display()))?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660))?;
    listener.set_nonblocking(true)?;
    log::info!("ipc listening on {}", socket_path.display());

    let hub = Arc::new(Mutex::new(Hub::default()));
    let mut threads = Vec::new();

    {
        let hub = hub.clone();
        let shutdown = shutdown.clone();
        threads.push(std::thread::Builder::new().name("ipc-broadcast".into()).spawn(move || {
            broadcaster(out, hub, shutdown)
        })?);
    }
    threads.push(std::thread::Builder::new().name("ipc-accept".into()).spawn(move || {
        acceptor(listener, hub, requests, shutdown)
    })?);
    Ok(IpcServer { threads })
}

fn broadcaster(out: Receiver<Out>, hub: Arc<Mutex<Hub>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        let Ok(message) = out.recv_timeout(Duration::from_millis(100)) else { continue };
        let (json, kind) = match &message {
            Out::Telemetry(snapshot) => (serde_json::to_string(snapshot), Kind::Plain),
            Out::Alarm(event) => (serde_json::to_string(event), Kind::Alarm(event.clone())),
            Out::Params(params) => (serde_json::to_string(params), Kind::Params),
            Out::MenuInput(button) => (Ok(protocol::menu_input(button)), Kind::Plain),
        };
        let Ok(json) = json else { continue };
        let json = Arc::new(json);
        let mut hub = hub.lock().unwrap();
        match kind {
            Kind::Alarm(AlarmEvent { id, active: true, .. }) => {
                hub.alarms.insert(id, json.clone());
            }
            Kind::Alarm(AlarmEvent { id, .. }) => {
                hub.alarms.remove(id);
            }
            Kind::Params => hub.params = Some(json.clone()),
            Kind::Plain => {}
        }
        hub.broadcast(json);
    }
}

enum Kind {
    Plain,
    Params,
    Alarm(AlarmEvent),
}

fn acceptor(
    listener: UnixListener,
    hub: Arc<Mutex<Hub>>,
    requests: Sender<ClientRequest>,
    shutdown: Arc<AtomicBool>,
) {
    let next_id = AtomicU64::new(1);
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                if let Err(error) = serve_client(id, stream, &hub, &requests) {
                    log::warn!("ipc client {id}: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                log::warn!("ipc accept: {error}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn serve_client(
    id: u64,
    stream: UnixStream,
    hub: &Arc<Mutex<Hub>>,
    requests: &Sender<ClientRequest>,
) -> Result<()> {
    stream.set_nonblocking(false)?;
    let (queue, outgoing) = bounded::<Arc<String>>(CLIENT_QUEUE);

    // Register under the lock so no event falls between the state snapshot and the first
    // broadcast
    {
        let mut guard = hub.lock().unwrap();
        guard.send_state(&queue);
        guard.clients.push(Client { id, queue: queue.clone(), stream: stream.try_clone()? });
    }
    log::info!("ipc client {id} connected");

    let mut writer = stream.try_clone()?;
    std::thread::Builder::new().name(format!("ipc-w{id}")).spawn(move || {
        for message in outgoing.iter() {
            if protocol::write_message(&mut writer, &message).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Both);
    })?;

    let hub = hub.clone();
    let requests = requests.clone();
    let mut reader = stream;
    std::thread::Builder::new().name(format!("ipc-r{id}")).spawn(move || {
        while let Ok(text) = protocol::read_message(&mut reader) {
            match Request::parse(&text) {
                Err(reply) => {
                    let _ = queue.try_send(Arc::new(reply));
                }
                Ok(Request::GetState { id: request_id }) => {
                    hub.lock().unwrap().send_state(&queue);
                    let _ = queue.try_send(Arc::new(ack(request_id, &Ok(()))));
                }
                Ok(request) => {
                    let request_id = request.id();
                    let sent = requests.try_send(ClientRequest { request, reply: queue.clone() });
                    if sent.is_err() {
                        let _ = queue.try_send(Arc::new(ack(request_id, &Err("daemon busy".into()))));
                    }
                }
            }
        }
        hub.lock().unwrap().clients.retain(|client| client.id != id);
        log::info!("ipc client {id} disconnected");
        // Dropping `queue` ends the writer thread
    })?;
    Ok(())
}

/// Blocking client helper shared by `ctl` and tests
pub fn send(stream: &mut UnixStream, json: &str) -> std::io::Result<()> {
    protocol::write_message(stream, json)?;
    stream.flush()
}
