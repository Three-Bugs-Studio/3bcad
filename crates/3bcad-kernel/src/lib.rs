//! B-Rep modelling bridge over the pure-Rust `truck` kernel.

pub mod boolean;
pub mod dome;
pub mod edge_ops;
pub mod fit;
pub mod hlr;
pub mod interference;
pub mod loft;
pub mod mass;
pub mod mate;
pub mod mesher;
pub mod offset;
pub mod operations;
pub mod print;
pub mod round;
pub mod skin;
pub mod solid;
pub mod sweep;
pub mod thread;
pub mod tool;
pub mod topology;
pub mod wrap;

pub use solid::{extract_solid, CompressedSolid, Plane, Solid};
pub use tool::{Draft, Tool, ToolFace};

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

thread_local!(static CATCHING: Cell<u32> = const { Cell::new(0) });
/// How many [`caught`] calls are running, on all threads.
static CATCHING_ANYWHERE: AtomicUsize = AtomicUsize::new(0);

/// `catch_unwind` for a panic 3BCAD expects and recovers from, such as truck on geometry it cannot
/// handle: while `f` runs, the crash reporter logs a panic in it instead of filing a crash.
pub fn caught<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    CATCHING.with(|c| c.set(c.get() + 1));
    CATCHING_ANYWHERE.fetch_add(1, Relaxed);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CATCHING_ANYWHERE.fetch_sub(1, Relaxed);
    CATCHING.with(|c| c.set(c.get() - 1));
    result
}

/// Whether a panic on this thread now would be caught by [`caught`]: this thread is inside it, or
/// it is one of truck's thread-pool workers, whose panic goes back to the thread that started the
/// work.
// shortcut: a worker counts as caught while any thread is inside `caught`, so a real panic in pool
// work started outside it at that moment is only logged; tag the pool's jobs if that ever matters.
pub fn catching() -> bool {
    CATCHING.with(|c| c.get() > 0)
        || (rayon_core::current_thread_index().is_some() && CATCHING_ANYWHERE.load(Relaxed) > 0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn caught_marks_only_its_own_span() {
        assert!(!super::catching());
        let r = super::caught(|| {
            assert!(super::caught(|| assert!(super::catching())).is_ok());
            assert!(super::catching(), "an inner catch keeps the outer one");
            panic!("expected");
        });
        assert!(r.is_err() && !super::catching());
        // Called off the pool, a scope runs on a pool worker, as truck's parallel meshing does.
        let on_worker = || rayon_core::scope(|_| super::catching());
        assert_eq!(super::caught(on_worker).ok(), Some(true));
    }
}
