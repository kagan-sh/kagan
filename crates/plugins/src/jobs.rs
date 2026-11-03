use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use async_channel::{Receiver, Sender};
use gpui_shell::{HostError, HostModule, HostValue};
use serde_json::{Map, Value};

use crate::area::{from_host, to_host};
use crate::page::MESSAGE_CAP;

type Job = Map<String, Value>;
type Reply = Result<Value, String>;

/// How many page jobs may wait for `next()` at once.
pub(crate) const PAGE_PENDING_CAP: usize = 32;
/// Total JSON bytes of those waiting page jobs.
pub(crate) const PAGE_PENDING_BYTES: usize = MESSAGE_CAP;

pub(crate) enum Ended {
    Unloaded,
    TimedOut,
}

struct PageBounds {
    pages: AtomicUsize,
    bytes: AtomicUsize,
}

impl PageBounds {
    fn try_add(&self, bytes: usize) -> bool {
        let pages = self.pages.load(Ordering::Relaxed);
        let held = self.bytes.load(Ordering::Relaxed);
        if pages >= PAGE_PENDING_CAP || held.saturating_add(bytes) > PAGE_PENDING_BYTES {
            return false;
        }
        self.pages.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        true
    }

    fn release(&self, bytes: usize) {
        self.pages.fetch_sub(1, Ordering::Relaxed);
        self.bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn bytes(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }
}

struct PageTicket {
    job: Job,
    alive: Weak<()>,
    /// True while this ticket still counts against the pending page cap.
    pending: Arc<AtomicBool>,
    bytes: usize,
    bounds: Arc<PageBounds>,
}

impl PageTicket {
    fn release_bounds(&self) {
        if self.pending.swap(false, Ordering::Relaxed) {
            self.bounds.release(self.bytes);
        }
    }
}

enum Queued {
    Job(Job),
    /// Page bodies live in [`Jobs::page_waiting`]; the channel carries only the id.
    Page(String),
}

/// One plugin's job queue. Never shared: gpui-shell does not cancel a pending `next()`
/// on unload, so a shared queue would hand the next plugin's job to a dead one.
pub(crate) struct Jobs {
    queue: (Sender<Queued>, Receiver<Queued>),
    open: RefCell<BTreeMap<String, Sender<Reply>>>,
    /// Page jobs still waiting for `next()`, so a timeout can drop their bodies.
    /// `Arc`/`Mutex` so the `Send` `next()` future can take a ticket without the `Rc`.
    page_waiting: Arc<Mutex<BTreeMap<String, PageTicket>>>,
    bounds: Arc<PageBounds>,
    next: Cell<u64>,
}

impl Jobs {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            queue: async_channel::unbounded(),
            open: RefCell::default(),
            page_waiting: Arc::new(Mutex::new(BTreeMap::new())),
            bounds: Arc::new(PageBounds {
                pages: AtomicUsize::new(0),
                bytes: AtomicUsize::new(0),
            }),
            next: Cell::new(1),
        })
    }

    /// Bytes of page payloads still held for `next()` (not yet timed out or taken).
    pub(crate) fn pending_page_bytes(&self) -> usize {
        self.bounds.bytes()
    }

    /// Queues `job` and returns its id and where its reply arrives. The reply channel
    /// closes without a value when the plugin unloads.
    pub(crate) fn send(&self, mut job: Job) -> Option<(String, Receiver<Reply>)> {
        let id = self.next_id();
        job.insert("id".into(), id.clone().into());
        let (reply, replied) = async_channel::bounded(1);
        self.open.borrow_mut().insert(id.clone(), reply);
        if self.queue.0.try_send(Queued::Job(job)).is_err() {
            self.forget(&id);
            return None;
        }
        Some((id, replied))
    }

    /// Queues a page job that dies with `alive`. Refuses when the pending page cap is full.
    pub(crate) fn send_page(
        &self,
        mut job: Job,
        alive: Weak<()>,
        bytes: usize,
    ) -> Option<(String, Receiver<Reply>)> {
        alive.upgrade()?;
        if !self.bounds.try_add(bytes) {
            return None;
        }
        let id = self.next_id();
        job.insert("id".into(), id.clone().into());
        let pending = Arc::new(AtomicBool::new(true));
        let (reply, replied) = async_channel::bounded(1);
        self.open.borrow_mut().insert(id.clone(), reply);
        self.page_waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                id.clone(),
                PageTicket {
                    job,
                    alive,
                    pending,
                    bytes,
                    bounds: self.bounds.clone(),
                },
            );
        if self.queue.0.try_send(Queued::Page(id.clone())).is_err() {
            self.free_page(&id);
            self.forget(&id);
            return None;
        }
        Some((id, replied))
    }

    pub(crate) fn forget(&self, id: &str) {
        self.open.borrow_mut().remove(id);
    }

    pub(crate) async fn settled(
        &self,
        id: &str,
        replied: &Receiver<Reply>,
        timer: impl Future<Output = ()>,
    ) -> Result<Reply, Ended> {
        match futures::future::select(pin!(replied.recv()), pin!(timer)).await {
            futures::future::Either::Left((reply, _)) => {
                self.page_waiting
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(id);
                self.forget(id);
                reply.map_err(|_| Ended::Unloaded)
            }
            futures::future::Either::Right(((), _)) => {
                self.free_page(id);
                self.forget(id);
                Err(Ended::TimedOut)
            }
        }
    }

    /// Rejects the plugin's pending `next()` and drops every open job.
    pub(crate) fn close(&self) {
        self.queue.0.close();
        self.open.borrow_mut().clear();
        let waiting = std::mem::take(
            &mut *self
                .page_waiting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for (_, ticket) in waiting {
            ticket.release_bounds();
        }
    }

    fn settle(&self, id: &str, reply: Reply) -> Result<HostValue, HostError> {
        self.page_waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id);
        let sender = self
            .open
            .borrow_mut()
            .remove(id)
            .ok_or_else(|| HostError::new(format!("no open job {id}")))?;
        let _ = sender.try_send(reply);
        Ok(HostValue::Null)
    }

    fn next_id(&self) -> String {
        self.next.replace(self.next.get() + 1).to_string()
    }

    fn free_page(&self, id: &str) {
        if let Some(ticket) = self
            .page_waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id)
        {
            ticket.release_bounds();
        }
    }

    pub(crate) fn module(self: &Rc<Self>) -> HostModule {
        let receiver = self.queue.1.clone();
        let waiting = self.page_waiting.clone();
        let (replies, failures) = (self.clone(), self.clone());
        HostModule::new("kagan/jobs")
            .declarations(
                "export interface Job { id: string; kind: string; [field: string]: unknown }\n\
                 export function next(): Promise<Job>;\n\
                 export function reply(id: string, value: unknown): void;\n\
                 export function fail(id: string, message: string): void;",
            )
            .async_function("next", move |_| {
                let receiver = receiver.clone();
                let waiting = waiting.clone();
                Ok(async move {
                    loop {
                        let queued = receiver
                            .recv()
                            .await
                            .map_err(|_| HostError::new("the plugin was unloaded"))?;
                        match queued {
                            Queued::Job(job) => return Ok(to_host(Value::Object(job))),
                            Queued::Page(id) => {
                                let Some(ticket) = waiting
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .remove(&id)
                                else {
                                    continue;
                                };
                                let live = ticket.alive.upgrade().is_some();
                                let waiting_cap = ticket.pending.load(Ordering::Relaxed);
                                if live && waiting_cap {
                                    ticket.release_bounds();
                                    return Ok(to_host(Value::Object(ticket.job)));
                                }
                                ticket.release_bounds();
                            }
                        }
                    }
                })
            })
            .function("reply", move |arguments| {
                replies.settle(
                    arguments.string(0)?,
                    Ok(from_host(arguments.value(1).unwrap_or(&HostValue::Null))),
                )
            })
            .function("fail", move |arguments| {
                failures.settle(arguments.string(0)?, Err(arguments.string(1)?.to_owned()))
            })
    }
}
