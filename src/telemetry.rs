pub fn init_subscriber() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .try_init()
        .ok();
}

pub fn init_test_subscriber() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .json()
        .with_test_writer()
        .try_init()
        .ok();
}
