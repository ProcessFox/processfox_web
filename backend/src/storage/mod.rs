//! S3-kompatibler Objektspeicher (MinIO self-hosted, AWS S3 optional —
//! CLAUDE.md §5/§6). Der Client wird konfiguriert, macht aber bis zum ersten
//! Request kein Netzwerk-I/O — der Start scheitert also nicht, wenn MinIO
//! noch hochfährt.

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;

use crate::config::S3Config;

#[derive(Clone)]
pub struct Storage {
    pub client: Client,
    pub bucket: String,
}

impl Storage {
    pub fn new(cfg: &S3Config) -> Self {
        let creds = Credentials::new(
            &cfg.access_key,
            &cfg.secret_key,
            None,
            None,
            "processfox-static",
        );
        let s3_cfg = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(Region::new(cfg.region.clone()))
            .endpoint_url(&cfg.endpoint)
            // MinIO spricht Path-Style (`endpoint/bucket/key`), nicht
            // Virtual-Hosted-Style.
            .force_path_style(true)
            .credentials_provider(creds)
            .build();

        Self {
            client: Client::from_conf(s3_cfg),
            bucket: cfg.bucket.clone(),
        }
    }
}
