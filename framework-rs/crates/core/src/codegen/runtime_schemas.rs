//! Resolve checked HTTP schema resources before emitting lightweight SDK validators.
//! Only known metadata schema slots are visited; literal JSON stays untouched.
use anyhow::{Context, Result};
use serde_json::Value;

pub(crate) fn schema(source: &Value) -> Result<Value> {
    Ok(crate::codegen::canonical_json(
        crate::codegen::schema::bundle_document_resources(source, "")?
            .unwrap_or_else(|| source.clone()),
    ))
}

fn field_values(value: &mut Value) -> Result<()> {
    for field in value
        .as_object_mut()
        .context("Invalid SDK field rules")?
        .values_mut()
    {
        field["schema"] = schema(&field["schema"])?;
    }
    Ok(())
}

pub(crate) fn fields(source: &str) -> Result<String> {
    let mut value: Value = serde_json::from_str(source)?;
    field_values(&mut value)?;
    Ok(crate::codegen::canonical_json(value).to_string())
}

fn request_groups(value: &mut Value) -> Result<()> {
    for group in value
        .as_object_mut()
        .context("Invalid SDK request groups")?
        .values_mut()
    {
        group["schema"] = schema(&group["schema"])?;
        field_values(&mut group["fields"])?;
    }
    Ok(())
}

pub(crate) fn request_objects(source: &str) -> Result<String> {
    if source.is_empty() {
        return Ok(String::new());
    }
    let mut value: Value = serde_json::from_str(source)?;
    request_groups(&mut value)?;
    Ok(crate::codegen::canonical_json(value).to_string())
}

pub(crate) fn versions(source: &str) -> Result<String> {
    if source.is_empty() {
        return Ok(String::new());
    }
    let mut value: Value = serde_json::from_str(source)?;
    for branch in value["branches"]
        .as_array_mut()
        .context("Invalid SDK versions")?
    {
        branch["bodySchema"] = schema(&branch["bodySchema"])?;
        for key in ["params", "query", "headers"] {
            field_values(&mut branch[key])?;
        }
        if let Some(groups) = branch.get_mut("requestObjects") {
            request_groups(groups)?;
        }
        for response in branch["responses"]
            .as_object_mut()
            .context("Invalid SDK responses")?
            .values_mut()
        {
            response["schema"] = schema(&response["schema"])?;
            field_values(&mut response["headers"])?;
        }
    }
    Ok(crate::codegen::canonical_json(value).to_string())
}
