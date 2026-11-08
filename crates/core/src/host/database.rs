use async_channel::Sender;
use gpui_kit::{BackgroundExecutor, Task};
use kg_domain::OpError;
use kg_store::Store;

pub(crate) type Result<T> = std::result::Result<T, OpError>;
type Job = Box<dyn FnOnce(&mut Store) + Send>;

#[derive(Clone)]
pub(crate) struct Database {
    send: Sender<Job>,
}

impl Database {
    pub(crate) fn new(mut store: Store, executor: &BackgroundExecutor) -> (Self, Task<()>) {
        let (send, receive) = async_channel::unbounded::<Job>();
        let task = executor.spawn(async move {
            while let Ok(job) = receive.recv().await {
                job(&mut store);
            }
        });
        (Self { send }, task)
    }

    pub(crate) async fn call<T: Send + 'static>(
        &self,
        call: impl FnOnce(&mut Store) -> kg_store::Result<T> + Send + 'static,
    ) -> Result<T> {
        let (send, receive) = async_channel::bounded(1);
        self.send
            .send(Box::new(move |store| {
                let result = call(store).map_err(|error| match error {
                    kg_store::Error::NotFound { what, id } => {
                        OpError::NotFound(format!("{what} {id} not found"))
                    }
                    kg_store::Error::Sqlite(error) => OpError::Failed(error.to_string()),
                    kg_store::Error::Conflict(message) => OpError::Invalid(message),
                    error @ (kg_store::Error::Future { .. }
                    | kg_store::Error::PreRelease { .. }
                    | kg_store::Error::Foreign { .. }
                    | kg_store::Error::Io(_)) => OpError::Failed(error.to_string()),
                });
                // The caller dropped its receiver, so nobody wants the result.
                let _ = send.try_send(result);
            }))
            .await
            .map_err(|_| OpError::Failed("database closed".into()))?;
        receive
            .recv()
            .await
            .map_err(|_| OpError::Failed("database closed".into()))?
    }
}
