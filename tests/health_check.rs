#[tokio::test]
async fn health_check_works() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind random port");
    let address = listener.local_addr().expect("failed to get local address");

    tokio::spawn(zero2prod::run(listener));

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{address}/health_check"))
        .send()
        .await
        .expect("failed to execute request");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
