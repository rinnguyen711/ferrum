//! Live S3 round-trip against MinIO. Skipped unless RUSTAPI_TEST_S3=1.
//! Expects env: S3_ENDPOINT, S3_BUCKET, S3_REGION, S3_KEY, S3_SECRET.

use bytes::Bytes;
use rustapi_media::s3::{S3Config, S3Provider};
use rustapi_media::StorageProvider;

#[tokio::test]
async fn s3_round_trip() {
    if std::env::var("RUSTAPI_TEST_S3").ok().as_deref() != Some("1") {
        eprintln!("skipping: set RUSTAPI_TEST_S3=1 to run");
        return;
    }
    let cfg = S3Config {
        bucket: std::env::var("S3_BUCKET").unwrap(),
        region: std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        endpoint: std::env::var("S3_ENDPOINT").ok(),
        access_key: std::env::var("S3_KEY").unwrap(),
        secret_key: std::env::var("S3_SECRET").unwrap(),
    };
    let p = S3Provider::new(cfg).unwrap();
    p.test().await.unwrap();
    p.put("it/x.txt", Bytes::from_static(b"hi"), "text/plain")
        .await
        .unwrap();
    assert_eq!(&p.get("it/x.txt").await.unwrap()[..], b"hi");
    p.delete("it/x.txt").await.unwrap();
}
