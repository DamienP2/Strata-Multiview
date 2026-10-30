//! Thread priorities for the playout. The channel clock, the render threads
//! and the output threads ask the system to run them ahead of ordinary work:
//! a clock tick or a frame that waits behind a backup, a browser or a
//! compile is a frame shown twice on air.
//!
//! - macOS: the "user interactive" class (no timer coalescing, performance
//!   cores), the class of the frontmost app's interface.
//! - Linux: the clock gets a real-time slot when the service may have one
//!   (root, CAP_SYS_NICE); the others keep the service's raised priority.
//! - Windows: higher thread priorities (time-critical for the clock).

/// What a thread does.
#[derive(Clone, Copy)]
pub enum Work {
    Clock,
    Render,
    Output,
}

/// Raises the priority of the calling thread (best effort: refused quietly).
pub fn prioritize(work: Work) {
    #[cfg(target_os = "macos")]
    {
        let _ = work;
        // SAFETY: sets the QoS class of the calling thread only.
        unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
        }
    }
    #[cfg(target_os = "linux")]
    {
        if matches!(work, Work::Clock) {
            // SAFETY: a plain C struct (musl's has more fields than glibc's),
            // then the calling thread's own scheduling; fails without the right.
            unsafe {
                let mut param: libc::sched_param = std::mem::zeroed();
                param.sched_priority = 10;
                libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &param);
            }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
            THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_TIME_CRITICAL,
        };
        let p = match work {
            Work::Clock => THREAD_PRIORITY_TIME_CRITICAL,
            Work::Output => THREAD_PRIORITY_HIGHEST,
            Work::Render => THREAD_PRIORITY_ABOVE_NORMAL,
        };
        // SAFETY: the pseudo-handle of the calling thread.
        unsafe {
            SetThreadPriority(GetCurrentThread(), p);
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let _ = work;
}
