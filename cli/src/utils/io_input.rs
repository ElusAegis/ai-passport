pub trait InputSource: Send + 'static {
    fn next(&mut self) -> anyhow::Result<Option<String>>;
}

// 2) Task-local holder
use anyhow::Context;
use dialoguer::console::{style, Term};
use std::io::stdin;
use std::sync::{Arc, Mutex};
use tracing::info;

tokio::task_local! {
    static INPUT_CTX: Arc<Mutex<dyn InputSource>>;
}

// 3) Helper to run a future with an injected source
pub async fn with_input_source<S, F, R>(src: S, fut: F) -> R
where
    S: InputSource,
    F: std::future::Future<Output = R>,
{
    let arc: Arc<Mutex<dyn InputSource>> = Arc::new(Mutex::new(src));
    INPUT_CTX.scope(arc, fut).await
}

// 4) Anywhere deep in your code:
pub(crate) fn try_read_user_input_from_ctx() -> Option<anyhow::Result<Option<String>>> {
    INPUT_CTX
        .try_with(|arc| {
            let mut guard = arc.lock().unwrap();
            guard.next()
        })
        .ok()
}

pub struct StdinInputSource;

impl InputSource for StdinInputSource {
    fn next(&mut self) -> anyhow::Result<Option<String>> {
        let term = Term::stdout();

        // Print the prompt via logging
        info!(target: "plain",
            "{}\n(type 'exit' to end): \n> ",
            style("💬 Your message").cyan().bold()
        );

        // Now reposition the cursor onto the "> " spot
        term.move_cursor_up(1).context("Failed to move cursor up")?;
        term.move_cursor_right(2)
            .context("Failed to move cursor right")?;

        let mut line = String::new();
        stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() || line.eq_ignore_ascii_case("exit") {
            Ok(None)
        } else {
            Ok(Some(line.to_string()))
        }
    }
}

pub struct VecInputSource {
    buf: std::vec::IntoIter<Option<String>>,
}
impl VecInputSource {
    pub fn new(lines: Vec<Option<String>>) -> Self {
        Self {
            buf: lines.into_iter(),
        }
    }
}
impl InputSource for VecInputSource {
    fn next(&mut self) -> anyhow::Result<Option<String>> {
        Ok(self.buf.next().flatten())
    }
}
