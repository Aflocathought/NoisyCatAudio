use crate::{Document, Edit, Store};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub maximum_ui_fps: u32,
    pub language: String,
    pub loaded: bool,
    pub pending: bool,
    pub error: Option<String>,
}
struct State {
    document: Document,
    pending: Vec<Edit>,
    revision: u64,
    loaded: bool,
    error: Option<String>,
    stopping: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

/// One worker per settings path per loaded module. Other DLLs/processes use
/// the same stable file lock. The last client joins before its DLL can unload.
pub struct Preferences {
    path: PathBuf,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
type Registry = Mutex<HashMap<PathBuf, Weak<Preferences>>>;
static CLIENTS: OnceLock<Registry> = OnceLock::new();

impl Preferences {
    pub fn open(path: PathBuf) -> std::io::Result<Arc<Self>> {
        let mut registry = CLIENTS.get_or_init(Default::default).lock().unwrap();
        registry.retain(|_, value| value.strong_count() > 0);
        if let Some(client) = registry.get(&path).and_then(Weak::upgrade) {
            return Ok(client);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                document: Document::default(),
                pending: vec![],
                revision: 0,
                loaded: false,
                error: None,
                stopping: false,
            }),
            wake: Condvar::new(),
        });
        let worker_shared = shared.clone();
        let worker_path = path.clone();
        let worker = thread::Builder::new()
            .name("audio-plugin-settings".into())
            .spawn(move || run(Store::new(worker_path), worker_shared))?;
        let client = Arc::new(Self {
            path: path.clone(),
            shared,
            worker: Some(worker),
        });
        registry.insert(path, Arc::downgrade(&client));
        Ok(client)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = self.shared.state.lock().unwrap();
        Snapshot {
            maximum_ui_fps: state.document.maximum_ui_fps(),
            language: state.document.language().into(),
            loaded: state.loaded,
            pending: !state.pending.is_empty(),
            error: state.error.clone(),
        }
    }
    pub fn set(&self, edit: Edit) {
        let mut state = self.shared.state.lock().unwrap();
        // UI changes become visible to sibling editors immediately. Pending
        // edits overlay external refreshes until the locked disk write succeeds.
        state.document.apply(std::slice::from_ref(&edit));
        if let Some(old) = state.pending.iter_mut().find(|old| old.same_key(&edit)) {
            *old = edit;
        } else {
            state.pending.push(edit);
        }
        state.revision = state.revision.wrapping_add(1);
        self.shared.wake.notify_one();
    }
}
impl Drop for Preferences {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().stopping = true;
        self.shared.wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(store: Store, shared: Arc<Shared>) {
    let mut closing_since = None;
    loop {
        let (edits, revision, stopping) = {
            let state = shared.state.lock().unwrap();
            (state.pending.clone(), state.revision, state.stopping)
        };
        if stopping {
            closing_since.get_or_insert_with(Instant::now);
        }
        let result = if edits.is_empty() {
            store.read()
        } else {
            store.update(&edits)
        };
        let mut state = shared.state.lock().unwrap();
        match result {
            Ok(mut document) => {
                // A newer same-key edit can arrive during I/O; acknowledge only
                // the exact edit that was written, then keep the newer overlay.
                state.pending.retain(|pending| !edits.contains(pending));
                document.apply(&state.pending);
                state.document = document;
                state.error = None;
                state.loaded = true;
            }
            Err(error) => {
                state.error = Some(error.to_string());
            }
        }
        if state.stopping
            && (state.pending.is_empty()
                || closing_since.is_some_and(|start| start.elapsed() >= Duration::from_millis(500)))
        {
            break;
        }
        if state.revision != revision || state.stopping != stopping {
            continue;
        }
        let delay = if state.pending.is_empty() {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(50)
        };
        let _ = shared.wake.wait_timeout(state, delay).unwrap();
    }
}
