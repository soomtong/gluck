//! Redirect stderr to /dev/null around a closure so that noisy third-party
//! libraries (e.g. hf-hub/indicatif progress bars) do not corrupt the
//! ratatui alternate-screen during a long-running operation.
//!
//! stdout is intentionally left alone: the TUI renders through fd 1, and
//! dup2-ing it from a background thread would race with the main thread's
//! terminal.draw() calls, making modal updates invisible to the user.
//!
//! On non-Unix targets (Windows) this is a no-op pass-through — hf-hub on
//! Windows does not write the same kind of progress bars to stderr that
//! corrupt the alternate screen, and replicating the dup2 dance with
//! `SetStdHandle` adds complexity without a known concrete benefit.

#[cfg(unix)]
mod imp {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    struct RestoreStderr(libc::c_int);

    impl Drop for RestoreStderr {
        fn drop(&mut self) {
            unsafe {
                libc::dup2(self.0, libc::STDERR_FILENO);
                libc::close(self.0);
            }
        }
    }

    pub fn with_silenced_stdio<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let devnull = match OpenOptions::new().write(true).open("/dev/null") {
            Ok(f) => f,
            Err(_) => return f(),
        };
        let dn = devnull.as_raw_fd();

        let saved_err = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved_err < 0 {
            return f();
        }

        let _guard = RestoreStderr(saved_err);

        unsafe {
            libc::dup2(dn, libc::STDERR_FILENO);
        }

        f()
    }
}

#[cfg(not(unix))]
mod imp {
    pub fn with_silenced_stdio<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f()
    }
}

pub use imp::with_silenced_stdio;
