//! Line protocol preserves lone UTF-16 surrogates without a JSON dependency.
use forklaunch_common::zod_regex::{
    RegexCompileError, RegexCompileLimits, RegexMatchLimits, ZodRegex,
};
use std::io::{self, BufRead};
fn units(value: &str) -> Vec<u16> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(',').map(|v| v.parse().expect("u16")).collect()
    }
}
fn main() {
    let mut cache: Option<(String, String, Result<ZodRegex, RegexCompileError>)> = None;
    for line in io::stdin().lock().lines() {
        let line = line.expect("line");
        let mut fields = line.split('\t');
        let source = fields.next().expect("source");
        let flags = fields.next().expect("flags");
        let text = units(fields.next().expect("text"));
        assert!(fields.next().is_none());
        if cache
            .as_ref()
            .is_none_or(|(p, f, _)| p != source || f != flags)
        {
            let result = ZodRegex::compile(
                &units(source),
                flags,
                RegexCompileLimits {
                    allocation_units: 16 << 20,
                    work: 10_000_000,
                    ..RegexCompileLimits::default()
                },
            );
            cache = Some((source.to_owned(), flags.to_owned(), result));
        }
        match &cache.as_ref().expect("cached").2 {
            Ok(regex) => match regex.test_utf16(&text, RegexMatchLimits::default()) {
                Ok(value) => println!("{value}"),
                Err(error) => println!("resource:{error:?}"),
            },
            Err(RegexCompileError::Resource(error)) => println!("resource:{error:?}"),
            Err(error) => println!("error:{error}"),
        }
    }
}
