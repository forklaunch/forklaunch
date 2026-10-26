use forklaunch_infrastructure_s3::AsyncObjectStore;
use forklaunch_infrastructure_s3::aws::environment::S3Environment;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--request") {
        let store = S3Environment::from_env()?.into_store().await?;
        let object = store.get("probe").await?.ok_or("Missing fixture object")?;
        println!("{}", serde_json::to_string(&json!({"body":object.body}))?);
        return Ok(());
    }
    let fixtures: Vec<Value> = serde_json::from_reader(std::io::stdin())?;
    let results=fixtures.iter().map(|fixture| {
        match S3Environment::from_vars(|name|fixture.get(name).and_then(Value::as_str).map(str::to_owned)) {
            Ok(settings)=>json!({"bucket":settings.store.bucket,"region":settings.store.region,"endpoint":settings.endpoint(),"staticCredentials":settings.static_credentials(),"prefix":settings.options.prefix,"uploadSeconds":settings.options.max_upload_lifetime.as_secs(),"downloadSeconds":settings.options.max_download_lifetime.as_secs()}),
            Err(error)=>json!({"error":error.code()}),
        }
    }).collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
