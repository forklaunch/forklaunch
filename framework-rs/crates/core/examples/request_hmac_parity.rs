use forklaunch_core::auth::request_hmac::{
    HmacAuthorization, SignedBody, create_hmac_token, generate_hmac_auth_header,
};
use serde::Deserialize;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Fixture {
    secret: String,
    method: String,
    path: String,
    kind: String,
    body: Vec<u8>,
    timestamp: String,
    nonce: String,
    signature: String,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixtures: Vec<Fixture> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for fixture in fixtures {
        let text = String::from_utf8_lossy(&fixture.body);
        let body = match fixture.kind.as_str() {
            "absent" => SignedBody::Absent,
            "text" => SignedBody::Text(&text),
            _ => SignedBody::Bytes(&fixture.body),
        };
        let signature = create_hmac_token(
            fixture.secret.as_bytes(),
            &fixture.method,
            &fixture.path,
            body,
            &fixture.timestamp,
            &fixture.nonce,
        )?;
        let header = generate_hmac_auth_header(
            fixture.secret.as_bytes(),
            "default",
            &fixture.method,
            &fixture.path,
            body,
            &fixture.timestamp,
            &fixture.nonce,
        )?;
        let provided = header.replace(&signature, &fixture.signature);
        let verified = HmacAuthorization::parse(&provided)?.verify(
            fixture.secret.as_bytes(),
            &fixture.method,
            &fixture.path,
            body,
        );
        output.push(serde_json::json!({"signature":signature,"header":header,"verified":verified}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
