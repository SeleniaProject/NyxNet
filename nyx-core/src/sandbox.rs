#![cfg(target_os = "linux")]

use anyhow::Result;
use seccomp::{SeccompAction, SeccompFilter, allow_syscall};

/// Install seccomp filter that kills the process on unexpected syscalls.
pub fn install_seccomp() -> Result<()> {
    let mut filter = SeccompFilter::new(SeccompAction::KillProcess)?;
    // Allow minimal set; extend as needed.
    allow_syscall!(filter, read, write, exit, futex, epoll_wait, epoll_ctl, epoll_create1,
                   clock_gettime, nanosleep, recvfrom, sendto, bind, connect, socket,
                   recvmsg, sendmsg, setsockopt, getsockopt, close, mmap, munmap,
                   rt_sigreturn, sigaltstack, getrandom, ioctl);

    filter.load()?;
    Ok(())
} 