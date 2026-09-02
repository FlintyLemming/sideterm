use crate::Pane;
use std::sync::Arc;
use std::time::{Duration, Instant};
use termwiz::surface::SequenceNo;

/// How long to wait for the spawned shell's first output before
/// giving up on injecting the workspace default command.
pub const DEFAULT_INJECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Abstraction over "a pane we might inject text into", so the timing
/// logic can be unit tested without a real terminal.
pub trait InjectTarget {
    fn current_seqno(&self) -> SequenceNo;
    fn is_dead(&self) -> bool;
    fn write_text(&self, text: &str) -> anyhow::Result<()>;
}

impl InjectTarget for Arc<dyn Pane> {
    fn current_seqno(&self) -> SequenceNo {
        self.get_current_seqno()
    }
    fn is_dead(&self) -> bool {
        (**self).is_dead()
    }
    fn write_text(&self, text: &str) -> anyhow::Result<()> {
        use std::io::Write;
        Ok(self.writer().write_all(text.as_bytes())?)
    }
}

/// Wait until `target` produces its first output (the shell has printed
/// its first prompt), then write `text` into it.
///
/// Writing immediately after spawn races the shell's startup
/// `tcflush()`, which would silently eat the injected text, so we poll
/// the terminal sequence number until it advances past the value
/// captured at spawn time.
///
/// Returns false (and writes nothing) if the target dies first or no
/// output appears within `timeout`.
pub async fn inject_text_after_first_output<T: InjectTarget>(
    target: &T,
    text: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> bool {
    let initial_seqno = target.current_seqno();
    let started = Instant::now();
    loop {
        if target.is_dead() {
            log::info!("command injection: pane died before first output; not injecting");
            return false;
        }
        if target.current_seqno() != initial_seqno {
            break;
        }
        if started.elapsed() >= timeout {
            log::info!("command injection: no output within {timeout:?}; not injecting");
            return false;
        }
        smol::Timer::after(poll_interval).await;
    }
    if let Err(err) = target.write_text(text) {
        log::error!("command injection: failed to write to pane: {err:#}");
        return false;
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct FakeTarget {
        seqno: AtomicUsize,
        dead: AtomicBool,
        written: Mutex<Vec<String>>,
    }

    impl FakeTarget {
        fn new() -> Self {
            Self {
                seqno: AtomicUsize::new(0),
                dead: AtomicBool::new(false),
                written: Mutex::new(vec![]),
            }
        }
    }

    impl InjectTarget for FakeTarget {
        fn current_seqno(&self) -> SequenceNo {
            self.seqno.load(Ordering::SeqCst)
        }
        fn is_dead(&self) -> bool {
            self.dead.load(Ordering::SeqCst)
        }
        fn write_text(&self, text: &str) -> anyhow::Result<()> {
            self.written.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    #[test]
    fn injects_after_first_output() {
        let target = Arc::new(FakeTarget::new());
        let bump = target.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            bump.seqno.fetch_add(1, Ordering::SeqCst);
        });
        let ok = smol::block_on(inject_text_after_first_output(
            &*target,
            "npm run dev\r",
            Duration::from_secs(5),
            Duration::from_millis(10),
        ));
        assert!(ok);
        assert_eq!(
            target.written.lock().unwrap().clone(),
            vec!["npm run dev\r".to_string()]
        );
    }

    #[test]
    fn gives_up_on_timeout() {
        let target = FakeTarget::new();
        let ok = smol::block_on(inject_text_after_first_output(
            &target,
            "npm run dev\r",
            Duration::from_millis(200),
            Duration::from_millis(10),
        ));
        assert!(!ok);
        assert!(target.written.lock().unwrap().is_empty());
    }

    #[test]
    fn gives_up_when_pane_is_dead() {
        let target = FakeTarget::new();
        target.dead.store(true, Ordering::SeqCst);
        let ok = smol::block_on(inject_text_after_first_output(
            &target,
            "npm run dev\r",
            Duration::from_secs(5),
            Duration::from_millis(10),
        ));
        assert!(!ok);
        assert!(target.written.lock().unwrap().is_empty());
    }
}
