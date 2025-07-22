#![cfg(target_os = "linux")]

use anyhow::Result;
use seccomp::{SeccompAction, SeccompFilter, allow_syscall};

/// Install seccomp filter that kills the process on unexpected syscalls.
pub fn install_seccomp() -> Result<()> {
    let mut filter = SeccompFilter::new(SeccompAction::KillProcess)?;
    // clang-format off (macro)
    allow_syscall!(filter,
        read, write, openat, close, fstat, statx, lseek, brk,
        mmap, mprotect, munmap, madvise,
        clone, clone3, set_robust_list, prctl, execve, exit, exit_group,
        futex, sched_yield, nanosleep, clock_gettime, getpid, gettid, tgkill, restart_syscall,
        socket, bind, connect, listen, accept4, recvfrom, sendto, recvmsg, sendmsg,
        setsockopt, getsockopt, shutdown,
        epoll_create1, epoll_ctl, epoll_wait,
        eventfd2, timerfd_create, timerfd_settime,
        pipe2, dup3,
        readlinkat, uname,
        getrandom, ioctl);
    // clang-format on

    filter.load()?;
    Ok(())
} 