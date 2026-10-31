use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;

use async_channel::{Receiver, Sender};
use gpui_shell::{HostError, HostModule, HostValue};
use serde_json::{Map, Value};

use crate::area::{from_host, to_host};

type Job = Map<String, Value>;
type Reply = Result<Value, String>;

pub(crate) enum Ended {
    Unloaded,
    TimedOut,
}

/// One plugin's job queue. Never shared: gpui-shell does not cancel a pending `next()`
/// on unload, so a shared queue would hand the next plugin's job to a dead one.
pub(crate) struct Jobs {
    queue: (Sender<Job>, Receiver<Job>),
    open: RefCell<BTreeMap<String, Sender<Reply>>>,
    next: Cell<u64>,
}

impl Jobs {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            queue: async_channel::unbounded(),
            open: RefCell::default(),
            next: Cell::new(1),
        })
    }

    /// Queues `job` and returns its id and where its reply arrives. The reply channel
    /// closes without a value when the plugin unloads.
    pub(crate) fn send(&self, mut job: Job) -> Option<(String, Receiver<Reply>)> {
        let id = self.next.replace(self.next.get() + 1).to_string();
        job.insert("id".into(), id.clone().into());
        let (reply, replied) = async_channel::bounded(1);
        self.open.borrow_mut().insert(id.clone(), reply);
        if self.queue.0.try_send(job).is_err() {
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
            futures::future::Either::Left((reply, _)) => reply.map_err(|_| Ended::Unloaded),
            futures::future::Either::Right(((), _)) => {
                self.forget(id);
                Err(Ended::TimedOut)
            }
        }
    }

    /// Rejects the plugin's pending `next()` and drops every open job.
    pub(crate) fn close(&self) {
        self.queue.0.close();
        self.open.borrow_mut().clear();
    }

    fn settle(&self, id: &str, reply: Reply) -> Result<HostValue, HostError> {
        let sender = self
            .open
            .borrow_mut()
            .remove(id)
            .ok_or_else(|| HostError::new(format!("no open job {id}")))?;
        let _ = sender.try_send(reply);
        Ok(HostValue::Null)
    }

    pub(crate) fn module(self: &Rc<Self>) -> HostModule {
        let receiver = self.queue.1.clone();
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
                Ok(async move {
                    let job = receiver
                        .recv()
                        .await
                        .map_err(|_| HostError::new("the plugin was unloaded"))?;
                    Ok(to_host(Value::Object(job)))
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
