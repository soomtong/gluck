//! Redirect stdout/stderr to /dev/null around a closure so that noisy
//! third-party libraries (e.g. hf-hub progress bars) do not corrupt the
//! ratatui alternate-screen during a long-running operation.

use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;

pub fn with_silenced_stdio<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let devnull = match OpenOptions::new().write(true).open("/dev/null") {
        Ok(f) => f,
        Err(_) => return f(),
    };
    let dn = devnull.as_raw_fd();

    let saved_out = unsafe { libc::dup(libc::STDOUT_FILENO) };
    let saved_err = unsafe { libc::dup(libc::STDERR_FILENO) };
    if saved_out < 0 || saved_err < 0 {
        if saved_out >= 0 {
            unsafe { libc::close(saved_out) };
        }
        if saved_err >= 0 {
            unsafe { libc::close(saved_err) };
        }
        return f();
    }

    unsafe {
        libc::dup2(dn, libc::STDOUT_FILENO);
        libc::dup2(dn, libc::STDERR_FILENO);
    }

    let result = f();

    unsafe {
        libc::dup2(saved_out, libc::STDOUT_FILENO);
        libc::dup2(saved_err, libc::STDERR_FILENO);
        libc::close(saved_out);
        libc::close(saved_err);
    }

    result
}
