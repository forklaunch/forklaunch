//! Explicit S3 client configuration. Construction never creates a bucket.
use forklaunch_core::{common::FrameworkResult, config::required, persistence::json::JsonRecordCodec};
use forklaunch_infrastructure_s3::{
    S3ObjectStoreConfig,
    aws::{AwsS3ObjectStore, aws_sdk_s3},
};

/// Customize the injected SDK client here for production credentials, TLS or retry policy.
pub fn object_store() -> FrameworkResult<AwsS3ObjectStore> {
    use aws_sdk_s3::config::{Credentials, Region};
    let region: String = required("S3_REGION")?;
    let bucket: String = required("S3_BUCKET")?;
    let client = aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::Config::builder()
            .region(Region::new(region.clone()))
            .credentials_provider(Credentials::new(
                required::<String>("S3_ACCESS_KEY_ID")?,
                required::<String>("S3_SECRET_ACCESS_KEY")?,
                None,
                None,
                "forklaunch-environment",
            ))
            .endpoint_url(required::<String>("S3_URL")?)
            .force_path_style(true)
            .build(),
    );
    AwsS3ObjectStore::new(
        client,
        S3ObjectStoreConfig::new(bucket).with_region(region),
        forklaunch_infrastructure_s3::aws::environment::S3Environment::from_env()?.options,
    )
}

/// Production/default-chain factory, also honoring local overrides, prefix and presign limits.
pub async fn configured_object_store() -> FrameworkResult<AwsS3ObjectStore> {
    forklaunch_infrastructure_s3::aws::environment::S3Environment::from_env()?.into_store().await
}

/// Explicit encrypted JSON factory using the AWS chain and ENCRYPTION_KEY.
pub async fn encrypted_object_store()
-> FrameworkResult<forklaunch_infrastructure_s3::json::JsonObjectStore<AwsS3ObjectStore>> {
    let codec = JsonRecordCodec::from_env()?;
    Ok(forklaunch_infrastructure_s3::json::JsonObjectStore::new(
        configured_object_store().await?,
        codec,
    ))
}
