#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    zero2prod::run_on("127.0.0.1:8000").await
}
