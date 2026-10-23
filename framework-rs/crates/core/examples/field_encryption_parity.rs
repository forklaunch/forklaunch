//! JSON harness used by the TypeScript/Rust field encryption conformance check.
use forklaunch_core::persistence::encryption::{CiphertextFormat, FieldEncryptor};
use serde_json::{Value, json};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let fixtures: Vec<Value> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for fixture in fixtures {
        let previous: Vec<String> = serde_json::from_value(fixture["previous"].clone())?;
        let format = if fixture["format"] == "v3" {
            CiphertextFormat::V3
        } else {
            CiphertextFormat::V2
        };
        let encryptor = FieldEncryptor::new(fixture["master"].as_str().ok_or("missing master")?)?
            .with_previous_keys(&previous)?
            .with_format(format);
        let plaintext = fixture["plaintext"].as_str().ok_or("missing plaintext")?;
        let tenant = fixture["tenant"].as_str().ok_or("missing tenant")?;
        let anon = fixture["anon"].as_str().ok_or("missing anon")?;
        let opened: Vec<_> = fixture["ciphertexts"].as_array().ok_or("missing ciphertexts")?.iter().map(|ciphertext| {
            let ciphertext = ciphertext.as_str().expect("fixture string");
            match encryptor.open(ciphertext, tenant) {
                Ok(value) => json!({"plaintext":value.plaintext,"keyId":value.key_id,"current":value.current,"version":value.version,"anon":value.anon,"stale":value.stale,"rotated":encryptor.rotate(ciphertext,tenant).expect("rotation")}),
                Err(_) => Value::Null,
            }
        }).collect();
        output.push(json!({"keyIds":encryptor.key_ids(),"encrypted":encryptor.encrypt(plaintext,tenant)?,"sealed":encryptor.seal(plaintext,tenant,anon)?,"blindIndex":encryptor.blind_index(plaintext,tenant)?,"blindIndexes":encryptor.blind_indexes(plaintext,tenant)?,"opened":opened}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
