use indicatif::{ProgressBar, ProgressStyle};
use std::{future::Future, io::IsTerminal, time::Duration};

/// Runs `work()` while showing a spinner with `msg`, then clears the line.
/// Works in TTY only; no output when stderr isn't a TTY.
pub async fn with_spinner<F, Fut, T, E>(msg: impl Into<String>, work: F) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let pb = if std::io::stderr().is_terminal() {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );
        pb.set_message(msg.into());
        pb.enable_steady_tick(Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    // RAII guard: always clear the spinner line on exit (success or error)
    struct Guard(Option<ProgressBar>);
    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(pb) = self.0.take() {
                pb.finish_and_clear();
            }
        }
    }
    let _g = Guard(pb);

    work().await
}

/// Same as above but takes an already-built future.
pub async fn with_spinner_future<Fut, T, E>(msg: impl Into<String>, fut: Fut) -> Result<T, E>
where
    Fut: Future<Output = Result<T, E>>,
{
    with_spinner(msg, || fut).await
}
