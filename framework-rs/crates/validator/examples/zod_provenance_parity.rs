use forklaunch_validator::{
    typescript_coerce::{OrderedJsonDocument, ZodJsonOutcome},
    zod_provenance::CompiledZodProvenance,
};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let before = input.clone();
        let compiled = CompiledZodProvenance::compile(input["provenance"].clone());
        let result = match compiled {
            Err(error) => json!({"registration":format!("{error:?}")}),
            Ok(compiled) => {
                let path: Vec<_> = input["path"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|part| part.as_str().unwrap())
                    .collect();
                let evaluated = if let Some(raw) = input["raw"].as_str() {
                    let document = OrderedJsonDocument::parse(raw.as_bytes()).unwrap();
                    compiled.evaluate_ordered(&document, &path)
                } else {
                    compiled.evaluate(input.get("value"), &path)
                };
                match evaluated {
                    Ok(ZodJsonOutcome::Valid(Some(value))) => {
                        json!({"ok":true,"absent":false,"value":value})
                    }
                    Ok(ZodJsonOutcome::Valid(None)) => json!({"ok":true,"absent":true}),
                    Ok(ZodJsonOutcome::Invalid(errors)) => json!({"ok":false,"errors":errors}),
                    Err(error) => json!({"unsupported":format!("{error:?}")}),
                }
            }
        };
        assert_eq!(input, before);
        println!("{result}");
    }
}
