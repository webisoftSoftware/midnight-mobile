//! A `block_on` that tolerates being re-entered on the same thread.
//!
//! `futures_executor::block_on` sets a thread-local "already inside an executor"
//! guard and panics with `EnterError` when a second call lands on the same
//! thread. The local prover re-enters exactly that way: `run_prove` drives its
//! proof inside `prover_pool().install(..)`, so a Rayon worker blocked on proof
//! A's future can, on running out of A's parallel work, steal the job that
//! drives proof B and start a second `block_on` on its own stack. Two admitted
//! proofs are enough, and `DEFAULT_CONCURRENT_PROOFS` is 2, so the shipped
//! configuration reaches it -- the panic escapes as `NativeInternal` and fails
//! every proof in the batch rather than one.
//!
//! Nothing in this runtime needs that guard. Every future it drives completes on
//! whatever thread polls it, so the executor below has no thread-local state and
//! nesting is just a nested call.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct ThreadWake(std::thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Drives `future` to completion on the calling thread, parking between polls.
///
/// Safe to call from inside another `block_on`, including from a Rayon worker
/// already driving a different future.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests;
