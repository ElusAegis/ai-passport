pub trait InputSource: Send + 'static {
    fn next(&mut self) -> anyhow::Result<Option<String>>;
}

// 2) Task-local holder
use std::sync::{Arc, Mutex};
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

pub(crate) struct StdinInputSource;

impl InputSource for StdinInputSource {
    fn next(&mut self) -> anyhow::Result<Option<String>> {
        use std::io::{self, Write};
        print!("\n💬 Your message\n(type 'exit' to end): \n> ");
        io::stdout().flush()?;

        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() || line.eq_ignore_ascii_case("exit") {
            Ok(None)
        } else {
            Ok(Some(line.to_string()))
        }
    }
}
