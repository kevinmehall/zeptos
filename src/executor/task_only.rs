use crate::Runtime;

/// Wrapper for placing a value that is not Send + Sync in a `static` but only
/// allowing it to be accessed from a task.
#[repr(transparent)]
pub struct TaskOnly<T>(T);

impl<T> TaskOnly<T> {
    /// Wrap a value.
    pub const fn new(v: T) -> Self where T: Send{
        TaskOnly(v)
    }

    /// Wrap a value.
    ///
    /// SAFETY: This is the equivalent of sending T to the
    /// task thread. You must either be on the task thread
    /// or T must be Send.
    pub const unsafe fn new_unsend(v: T) -> Self {
        TaskOnly(v)
    }

    /// Get the wrapped value.
    ///
    /// SAFETY: must only be called from inside a task,
    /// and not another core or an ISR at higher privilige.
    pub const unsafe fn get_unchecked(&self) -> &T {
        &self.0
    }

    /// Get the wrapped value.
    pub const fn get(&self, _runtime: Runtime) -> &T {
        unsafe { self.get_unchecked() }
    }
}

unsafe impl<T> Send for TaskOnly<T> {}
unsafe impl<T> Sync for TaskOnly<T> {}
