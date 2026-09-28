//! Per-window high-resolution wakeups. The helper only posts a coalesced
//! message; all UI/GPU work remains on the host's window thread.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND},
    System::Threading::*,
    UI::WindowsAndMessaging::*,
};

pub(super) const FRAME_MESSAGE: u32 = WM_APP + 0x432;
pub(super) struct FrameClock {
    stop: Arc<AtomicBool>,
    pub pending: Arc<AtomicBool>,
    pub running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl FrameClock {
    pub fn start(hwnd: HWND) -> Option<Self> {
        // SAFETY: unnamed timer with no borrowed callback state. Ownership is
        // transferred to the helper and reclaimed before the HWND is destroyed.
        let timer = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if timer.is_null() {
            return None;
        }
        let timer = timer as usize;
        let hwnd = hwnd as usize;
        let stop = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_pending = Arc::clone(&pending);
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let worker =
            std::thread::Builder::new().name("baseview-frame-clock".into()).spawn(move || {
                let timer = timer as *mut std::ffi::c_void;
                let hwnd = hwnd as HWND;
                while !worker_stop.load(Ordering::Acquire) {
                    // A one-shot wait avoids a global timeBeginPeriod setting.
                    // Hidden windows sleep longer and receive no frame messages.
                    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
                    let due: i64 = if visible { -10_000 } else { -500_000 };
                    // SAFETY: this thread exclusively owns the timer handle.
                    if unsafe { SetWaitableTimer(timer, &due, 0, None, std::ptr::null(), 0) } == 0 {
                        break;
                    }
                    unsafe {
                        WaitForSingleObject(timer, 100);
                    }
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if visible && !worker_pending.swap(true, Ordering::AcqRel) {
                        // One outstanding message bounds queue growth when the
                        // host is busy or a frame takes longer than its budget.
                        if unsafe { PostMessageW(hwnd, FRAME_MESSAGE, 0, 0) } == 0 {
                            worker_pending.store(false, Ordering::Release);
                        }
                    }
                }
                worker_running.store(false, Ordering::Release);
                unsafe {
                    CloseHandle(timer);
                }
            });
        match worker {
            Ok(worker) => Some(Self { stop, pending, running, worker: Some(worker) }),
            Err(_) => {
                unsafe {
                    CloseHandle(timer as *mut std::ffi::c_void);
                }
                None
            }
        }
    }
}
impl Drop for FrameClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
