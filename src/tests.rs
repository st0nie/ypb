use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode; // Use reqwest's StatusCode since we're testing with reqwest client
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::time;

use crate::{
    create_app_router,
    util::{AppState, Args},
};

// Helper function to start a test server
async fn start_test_server(temp_dir: &TempDir) -> (SocketAddr, Arc<AppState>) {
    // Create test Args with a small file size limit for testing
    let args = Args {
        port: 0, // Use port 0 to let the OS assign a free port
        file_path: temp_dir.path().to_string_lossy().into_owned(),
        clean_period: 3600,
        limit_size: 1024 * 10, // 10KB limit for testing
        syntax_theme: "vs".to_string(),
    };

    let app_state = Arc::new(AppState { args: args.clone() });

    // Create the router using the application's function
    let app = create_app_router(app_state.clone());

    // Bind to localhost with port 0 (OS-assigned port)
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    time::sleep(Duration::from_millis(100)).await;

    (addr, app_state)
}

// Helper function to extract a secret from the response of a PUT request
fn extract_secret(response: &str) -> Option<String> {
    response
        .lines()
        .find(|line| line.starts_with("secret:"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[1].trim().to_string())
            } else {
                None
            }
        })
}

// Helper function to extract a hash from the response of a PUT request
fn extract_hash(response: &str) -> Option<String> {
    response
        .lines()
        .find(|line| line.starts_with("short:"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                Some(parts[1].trim().to_string())
            } else {
                None
            }
        })
}

#[tokio::test]
async fn test_hash_format() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Test data
    let test_content = "1232\n";

    // 1. Upload content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(test_content)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    println!("PUT response: {}", body);

    // Extract the hash from the response
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // 2. Check the hash format 
    assert_eq!(hash, "K8mw");
}

#[tokio::test]
async fn test_upload_and_retrieve_success() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Test data
    let test_content = "Hello, this is a test!";

    // 1. Upload content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(test_content.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    println!("PUT response: {}", body);

    // Extract the hash from the response
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // 2. Retrieve the content
    let res = client
        .get(&format!("http://{addr}/{hash}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let retrieved_content = res.text().await.unwrap();

    // Verify the content matches what we uploaded
    assert_eq!(retrieved_content, test_content);
}

#[tokio::test]
async fn test_upload_and_delete_success() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Test data
    let test_content = "Content to be deleted";

    // 1. Upload content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(test_content.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();

    // Extract the hash and secret from the response
    let hash = extract_hash(&body).expect("Failed to extract hash from response");
    let secret = extract_secret(&body).expect("Failed to extract secret from response");

    // 2. Delete the content with correct secret
    let res = client
        .delete(&format!("http://{addr}/{hash}"))
        .body(secret)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // 3. Verify the content is deleted (should get 404)
    let res = client
        .get(&format!("http://{addr}/{hash}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_syntax_highlighting() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Test data - Rust code
    let test_content = r#"
    fn main() {
        println!("Hello, world!");
    }
    "#;

    // 1. Upload content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(test_content.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // 2. Retrieve content with syntax highlighting by adding .rs extension
    let res = client
        .get(&format!("http://{addr}/{hash}.rs"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Verify response contains code tag with the proper language class
    let html_content = res.text().await.unwrap();
    println!("HTML content received: {}", html_content); // Debug output

    // Just check for the code tag and content presence instead of highlight.js
    assert!(html_content.contains("<code class=\"rs\">"));
    assert!(html_content.contains(test_content.trim()));
}

#[tokio::test]
async fn test_not_found_error() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Try to get a non-existent hash
    let client = reqwest::Client::new();
    let res = client
        .get(&format!("http://{addr}/non_existent_hash"))
        .send()
        .await
        .unwrap();

    // Should receive a 404 Not Found
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let error_message = res.text().await.unwrap();
    assert_eq!(error_message, "File not found");
}

#[tokio::test]
async fn test_delete_with_wrong_secret() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Test data
    let test_content = "Content with protected deletion";

    // 1. Upload content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(test_content.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // 2. Try to delete with wrong secret
    let res = client
        .delete(&format!("http://{addr}/{hash}"))
        .body("wrong_secret")
        .send()
        .await
        .unwrap();

    // Should receive a 403 Forbidden
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let error_message = res.text().await.unwrap();
    assert_eq!(error_message, "Permission denied");

    // 3. Verify the content is still there
    let res = client
        .get(&format!("http://{addr}/{hash}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let retrieved_content = res.text().await.unwrap();
    assert_eq!(retrieved_content, test_content);
}

#[tokio::test]
async fn test_file_size_limit() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, app_state) = start_test_server(&temp_dir).await;

    // Create content that exceeds the size limit (10KB)
    let oversized_content = "a".repeat(app_state.args.limit_size + 1);

    // Try to upload oversized content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(oversized_content)
        .send()
        .await
        .unwrap();

    // Should receive a 413 Payload Too Large
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn test_empty_file_upload() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Upload empty content
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body("")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // Retrieve the empty content
    let res = client
        .get(&format!("http://{addr}/{hash}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let retrieved_content = res.text().await.unwrap();
    assert_eq!(retrieved_content, "");
}

#[tokio::test]
async fn test_url_redirect() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Upload a URL as content
    let url_content = "https://example.com";

    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(url_content.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // Get the URL content - this should trigger a redirect
    // Note: reqwest follows redirects by default, so we need to configure it not to
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let res = client
        .get(&format!("http://{addr}/{hash}"))
        .send()
        .await
        .unwrap();

    // Should receive a 307 Temporary Redirect
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

    // Verify that the redirect location is correct
    let redirect_location = res.headers().get("location").unwrap();
    assert_eq!(redirect_location, url_content);
}

#[tokio::test]
async fn test_binary_data_handling() {
    // Create a temporary directory for file storage
    let temp_dir = tempfile::tempdir().unwrap();
    let (addr, _) = start_test_server(&temp_dir).await;

    // Create some binary data (a simple PNG-like header)
    let binary_data = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // Chunk length
        0x49, 0x48, 0x44, 0x52, // "IHDR" chunk
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // Some random data
    ];

    // Upload binary data
    let client = reqwest::Client::new();
    let res = client
        .put(&format!("http://{addr}"))
        .body(binary_data.clone())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let body = res.text().await.unwrap();
    let hash = extract_hash(&body).expect("Failed to extract hash from response");

    // Retrieve the binary data with a PNG extension
    let res = client
        .get(&format!("http://{addr}/{hash}.png"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Check content-type is image/png
    assert_eq!(res.headers().get("content-type").unwrap(), "image/png");

    // Verify the binary data is correctly returned
    let retrieved_data = res.bytes().await.unwrap();
    assert_eq!(retrieved_data.len(), binary_data.len());
    assert_eq!(&retrieved_data[..], &binary_data[..]);
}
