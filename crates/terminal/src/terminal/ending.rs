use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

#[cfg(unix)]
use std::os::fd::RawFd;

use portable_pty::{Child, MasterPty};

use super::{Outgoing, Shared, lock};

pub(super) struct Ending {
    pub(super) child: Box<dyn Child + Send + Sync>,
    pub(super) master: Option<Box<dyn MasterPty + Send>>,
    pub(super) input: Sender<Outgoing>,
    pub(super) writer: Option<JoinHandle<()>>,
    pub(super) reader: Option<JoinHandle<()>>,
    pub(super) shared: Shared,
    #[cfg(unix)]
    pub(super) write_fd: RawFd,
    #[cfg(unix)]
    pub(super) write_closed: Arc<AtomicBool>,
}

impl Ending {
    pub(super) fn finish(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Before Windows build 26100, ClosePseudoConsole waits until the host
        // exits. The host stays in that wait while its cursor query is
        // unanswered or its last frame is unread. Answer once more now that
        // the child is dead. The reader thread is still running here, so it
        // can drain the final frame while the console closes.
        #[cfg(windows)]
        let _ = self.input.send(Outgoing::Bytes(b"\x1b[1;1R".to_vec()));
        // A checked write stuck on a full pty never reaches the close message.
        // Closing that writer fd unblocks write_all so we can join.
        #[cfg(unix)]
        super::close_writer_fd(self.write_fd, &self.write_closed);
        let _ = self.input.send(Outgoing::Bytes(Vec::new()));
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        self.master.take();
        if let Some(reader) = self.reader.take() {
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
