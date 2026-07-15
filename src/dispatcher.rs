use std::{
    sync::mpsc::sync_channel,
    thread::{self, ThreadId},
};

pub(crate) type MainThreadJob = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum DispatchError<E> {
    Schedule(E),
    Action(E),
    CompletionDropped,
}

pub(crate) fn dispatch_sync<T, E, F, S>(
    main_thread: ThreadId,
    schedule: S,
    action: F,
) -> Result<T, DispatchError<E>>
where
    T: Send + 'static,
    E: Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
    S: FnOnce(MainThreadJob) -> Result<(), E>,
{
    if thread::current().id() == main_thread {
        return action().map_err(DispatchError::Action);
    }

    let (completion_tx, completion_rx) = sync_channel(1);
    let job = Box::new(move || {
        let _ = completion_tx.send(action());
    });

    schedule(job).map_err(DispatchError::Schedule)?;
    let completion = completion_rx
        .recv()
        .map_err(|_| DispatchError::CompletionDropped)?;
    completion.map_err(DispatchError::Action)
}

#[cfg(test)]
mod tests {
    use super::{dispatch_sync, DispatchError};
    use std::thread;

    #[test]
    fn main_thread_dispatch_executes_directly() {
        let value = dispatch_sync(
            thread::current().id(),
            |_| -> Result<(), &'static str> { panic!("main-thread work must not be queued") },
            || Ok(42),
        );

        assert_eq!(value, Ok(42));
    }

    #[test]
    fn off_main_dispatch_returns_action_success() {
        let main_thread = thread::current().id();
        let result = thread::spawn(move || {
            dispatch_sync(
                main_thread,
                |job| {
                    job();
                    Ok::<(), &'static str>(())
                },
                || Ok(42),
            )
        })
        .join()
        .unwrap();

        assert_eq!(result, Ok(42));
    }

    #[test]
    fn off_main_dispatch_returns_action_error() {
        let main_thread = thread::current().id();
        let result = thread::spawn(move || {
            dispatch_sync(
                main_thread,
                |job| {
                    job();
                    Ok::<(), &'static str>(())
                },
                || Err::<(), _>("action failed"),
            )
        })
        .join()
        .unwrap();

        assert_eq!(result, Err(DispatchError::Action("action failed")));
    }

    #[test]
    fn queue_failure_is_returned_without_waiting() {
        let main_thread = thread::current().id();
        let result = thread::spawn(move || {
            dispatch_sync(
                main_thread,
                |_| Err("queue failed"),
                || Ok::<_, &'static str>(42),
            )
        })
        .join()
        .unwrap();

        assert_eq!(result, Err(DispatchError::Schedule("queue failed")));
    }

    #[test]
    fn dropped_queued_job_returns_completion_error() {
        let main_thread = thread::current().id();
        let result = thread::spawn(move || {
            dispatch_sync(
                main_thread,
                |job| {
                    drop(job);
                    Ok::<(), &'static str>(())
                },
                || Ok(42),
            )
        })
        .join()
        .unwrap();

        assert_eq!(result, Err(DispatchError::CompletionDropped));
    }

    #[test]
    fn queued_action_panic_propagates() {
        let main_thread = thread::current().id();
        let result = thread::spawn(move || {
            dispatch_sync(
                main_thread,
                |job| {
                    job();
                    Ok::<(), &'static str>(())
                },
                || -> Result<(), &'static str> { panic!("boom") },
            )
        })
        .join();

        assert!(result.is_err());
    }
}
