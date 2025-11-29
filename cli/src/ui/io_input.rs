use crate::providers::budget::ByteBudget;
use anyhow::Context;
use dialoguer::console::{style, Term};
use std::io::stdin;
use std::sync::{Arc, Mutex};
use tracing::info;

/// Input source trait for reading user input.
/// Implementations decide how to handle budget display.
pub trait InputSource: Send + 'static {
    fn next(&mut self, budget: &ByteBudget) -> anyhow::Result<Option<String>>;
}

tokio::task_local! {
    static INPUT_CTX: Arc<Mutex<dyn InputSource>>;
}

/// Helper to run a future with an injected input source.
pub async fn with_input_source<S, F, R>(src: S, fut: F) -> R
where
    S: InputSource,
    F: std::future::Future<Output = R>,
{
    let arc: Arc<Mutex<dyn InputSource>> = Arc::new(Mutex::new(src));
    INPUT_CTX.scope(arc, fut).await
}

/// Read user input with budget information displayed in the prompt.
pub(crate) fn try_read_user_input_with_budget(
    budget: &ByteBudget,
) -> anyhow::Result<Option<String>> {
    INPUT_CTX
        .try_with(|arc| {
            let mut guard = arc.lock().unwrap();
            guard.next(budget)
        })
        .map_err(|_| anyhow::anyhow!("No input source in context"))?
}

/// Format a byte count for human-readable display.
fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Threshold below which budget is considered critically low.
/// Since overhead calculations are estimates, anything under 100 bytes
/// is effectively unusable.
const LOW_BUDGET_THRESHOLD: usize = 100;

/// Format budget info for display with send (↑) and receive (↓) indicators.
/// Low values (under 100 bytes) are highlighted in red to warn user.
fn format_budget_info(send_bytes: usize, recv_bytes: usize) -> String {
    let send_display = if send_bytes < LOW_BUDGET_THRESHOLD {
        style(format_bytes(send_bytes)).red().to_string()
    } else {
        format_bytes(send_bytes)
    };
    let recv_display = if recv_bytes < LOW_BUDGET_THRESHOLD {
        style(format_bytes(recv_bytes)).red().to_string()
    } else {
        format_bytes(recv_bytes)
    };
    format!("[↑ {} | ↓ {}]", send_display, recv_display)
}

/// Check if budget is critically low (either send or receive under threshold).
fn is_budget_exhausted(send_bytes: usize, recv_bytes: usize) -> bool {
    send_bytes < LOW_BUDGET_THRESHOLD || recv_bytes < LOW_BUDGET_THRESHOLD
}

/// Standard input source that reads from stdin.
/// Shows budget info in the prompt when available.
pub struct StdinInputSource;

impl InputSource for StdinInputSource {
    fn next(&mut self, budget: &ByteBudget) -> anyhow::Result<Option<String>> {
        let term = Term::stdout();

        // Build prompt with optional budget info and exhaustion warning
        let (budget_suffix, exhaustion_warning) =
            match (budget.available_input_bytes(), budget.available_recv_bytes()) {
                (Some(send), Some(recv)) => {
                    let suffix = format!(" {}", style(format_budget_info(send, recv)).dim());
                    let warning = if is_budget_exhausted(send, recv) {
                        format!(
                            "\n{}",
                            style("⚠ Budget exhausted - type 'exit' to end session")
                                .red()
                                .bold()
                        )
                    } else {
                        String::new()
                    };
                    (suffix, warning)
                }
                _ => (String::new(), String::new()),
            };

        info!(
            target: "plain",
            "{}{}{}\n(type 'exit' to end): \n> ",
            style("💬 Your message").cyan().bold(),
            budget_suffix,
            exhaustion_warning
        );

        // Reposition cursor onto the "> " spot
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

/// Vector-based input source for testing.
/// Ignores budget info (not displayed in tests).
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
    fn next(&mut self, _budget: &ByteBudget) -> anyhow::Result<Option<String>> {
        Ok(self.buf.next().flatten())
    }
}
