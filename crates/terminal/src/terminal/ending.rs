use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use portable_pty::{Child, MasterPty};

use super::shared::{Shared, lock};
use super::stream::Outgoing;

pub(super) struct Ending {
    pub(super) child: Box<dyn Child + Send + Sync>,
    pub(super) master: Option<Box<dyn MasterPty + Send>>,
    pub(super) input: Sender<Outgoing>,
    pub(super) writer: Option<JoinHandle<()>>,
    pub(super) reader: Option<JoinHandle<()>>,
    pub(super) shared: Shared,
    pub(super) write_stop: Arc<AtomicBool>,
}

impl Ending {
    pub(super) fn finish(mut self) {
        // The program may already have exited.
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Before Windows build 26100, ClosePseudoConsole waits until the host
        // exits. The host stays in that wait while its cursor query is
        // unanswered or its last frame is unread. Answer once more now that
        // the child is dead. The reader thread is still running here, so it
        // can drain the final frame while the console closes.
        #[cfg(windows)]
        // The writer may already be gone.
        let _ = self.input.send(Outgoing::Reply(b"\x1b[1;1R".to_vec()));
        // A checked write waiting on a full pty must stop before we join. The
        // writer polls write_stop between non-blocking writes on Unix.
        self.write_stop.store(true, Ordering::Release);
        // An empty write wakes the writer so it sees `write_stop`; it may already be gone.
        let _ = self.input.send(Outgoing::Reply(Vec::new()));
        if let Some(writer) = self.writer.take() {
            // A panicked writer has nothing more to say; the screen is read below anyway.
            let _ = writer.join();
        }
        self.master.take();
        if let Some(reader) = self.reader.take() {
            // A panicked reader has nothing more to say; the screen is read below anyway.
            let _ = reader.join();
        }
        // The reader has read to the end, so nothing else will arrive: the text is final.
        #[cfg(test)]
        if let Some(gate) = lock(&self.shared.publish_gate).take() {
            let _ = gate.recv();
        }
        let mut screen = lock(&self.shared.screen);
        self.shared.drain(&mut screen);
        screen.end_hold();
        self.shared.publish(&screen);
        drop(screen);
        self.shared.settled.store(true, Ordering::Release);
    }
}
