pub(crate) fn init_logging() {
    // Init logging using tracing subscriber with ENV and some backup default
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();
}
