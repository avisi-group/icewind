use {
    log::trace,
    std::{
        sync::mpsc::{self, Receiver, Sender},
        thread::{self, JoinHandle},
    },
};

enum Event {
    /// Thread has finished setup
    Ready,
    /// Thread should shutdown
    Exit,
}

pub struct ThreadHandle {
    handle: JoinHandle<()>,
    tx: Sender<Event>,
    _rx: Receiver<Event>,
}

impl ThreadHandle {
    pub fn spawn<F: FnOnce(Context) + Send + 'static>(f: F) -> Self {
        let (thread_tx, thread_rx) = mpsc::channel();
        let (parent_tx, parent_rx) = mpsc::channel();

        let context = Context {
            tx: thread_tx,
            rx: parent_rx,
        };

        let handle = thread::spawn(|| f(context));

        let Ok(Event::Ready) = thread_rx.recv() else {
            panic!("Failed to receive ready event from spawned thread")
        };

        Self {
            handle,
            tx: parent_tx,
            _rx: thread_rx,
        }
    }

    pub fn exit(self) {
        trace!("sending exit");
        self.tx.send(Event::Exit).unwrap();
        trace!("waiting on join");
        self.handle.join().unwrap();
        trace!("joined");
    }
}

pub struct Context {
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Context {
    pub fn ready(&self) {
        self.tx.send(Event::Ready).unwrap();
    }

    pub fn received_exit(&self) -> bool {
        matches!(self.rx.try_recv(), Ok(Event::Exit))
    }
}
