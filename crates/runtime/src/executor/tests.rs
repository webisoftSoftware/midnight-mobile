use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use super::*;

/// Pending on its first poll, ready on the second, so the park/unpark path runs.
struct WakeOnce(bool);

impl Future for WakeOnce {
    type Output = u8;

    fn poll(mut self: std::pin::Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            Poll::Ready(9)
        } else {
            self.0 = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[test]
fn drives_pending_futures_to_completion() {
    assert_eq!(block_on(WakeOnce(false)), 9);
}

/// The condition `futures_executor::block_on` rejects with `EnterError`: a second
/// `block_on` starting on a thread that is already inside one.
#[test]
fn tolerates_being_nested_on_the_same_thread() {
    let nested = block_on(async {
        let inner = block_on(async { block_on(WakeOnce(false)) });
        inner + block_on(async { 1_u8 })
    });
    assert_eq!(nested, 10);
}

/// The shape that fails in production (see the module comment and issue #136): two
/// proofs admitted concurrently into one Rayon pool, each driving its future inside
/// `install`, with parallel work inside the future so workers run dry and steal the
/// other proof's job. Every round must complete; with `futures_executor::block_on`
/// this panics out of the pool instead.
#[test]
fn survives_two_concurrent_pool_admissions_that_steal_each_other() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let completed = AtomicUsize::new(0);
    let admit = |seed: usize| {
        pool.install(|| {
            block_on(async {
                // Uneven per-item work makes some workers finish early and steal.
                let total: usize = (0..64_usize)
                    .into_par_iter()
                    .map(|item| {
                        let spin = (item * seed) % 97;
                        (0..spin).fold(item, |accumulator, step| accumulator ^ step)
                    })
                    .sum();
                block_on(async move { total })
            })
        })
    };

    for round in 0..16 {
        rayon::join(
            || {
                admit(round + 1);
                completed.fetch_add(1, Ordering::Relaxed);
            },
            || {
                admit(round + 2);
                completed.fetch_add(1, Ordering::Relaxed);
            },
        );
    }

    assert_eq!(completed.load(Ordering::Relaxed), 32);
}
