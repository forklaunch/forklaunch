#![cfg(feature = "mappers")]
use forklaunch_core::{
    common::{FrameworkError, FrameworkResult},
    mappers::CheckedMapper,
};
use std::{
    future::ready,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(serde::Serialize, schemars::JsonSchema)]
struct Dto {
    #[schemars(length(min = 2, max = 8))]
    name: String,
    #[schemars(range(min = 1, max = 100))]
    revision: u32,
}
fn dto(name: &str) -> Dto {
    Dto {
        name: name.into(),
        revision: 1,
    }
}
#[tokio::test]
async fn request_rejects_before_mapper_side_effect_and_returns_original_value() {
    let checked = CheckedMapper::<Dto>::new().unwrap();
    let called = AtomicBool::new(false);
    let result = checked
        .to_entity(dto(""), |value| {
            called.store(true, Ordering::Relaxed);
            ready(Ok(value.name))
        })
        .await;
    assert!(result.is_err());
    assert!(!called.load(Ordering::Relaxed));
    assert_eq!(
        checked
            .to_entity(dto("valid"), |value| ready(Ok(value.name)))
            .await
            .unwrap(),
        "valid"
    );
}
#[tokio::test]
async fn response_rejects_invalid_mapping_and_preserves_business_errors() {
    let checked = CheckedMapper::<Dto>::new().unwrap();
    assert!(checked.to_dto((), |()| ready(Ok(dto("")))).await.is_err());
    assert!(
        checked
            .to_dto((), |()| ready(Ok(Dto {
                name: "valid".into(),
                revision: 101
            })))
            .await
            .is_err()
    );
    let failure = checked
        .to_dto((), |()| {
            ready(Err(FrameworkError::new("business_error", "do not erase")))
        })
        .await;
    assert_eq!(failure.err().unwrap().code(), "business_error");
    let good = checked
        .to_dto("valid", |name| ready(Ok(dto(name))))
        .await
        .unwrap();
    assert_eq!(good.name, "valid");
}
#[forklaunch_core::contract::schema]
struct NativeDto {
    #[field(min_length = 2, max_length = 8)]
    name: String,
}
#[tokio::test]
async fn native_schema_constraints_apply_outside_http() -> FrameworkResult<()> {
    let checked = CheckedMapper::<NativeDto>::new()?;
    assert!(
        checked
            .to_entity(
                NativeDto {
                    name: String::new()
                },
                |value| ready(Ok(value.name))
            )
            .await
            .is_err()
    );
    Ok(())
}
