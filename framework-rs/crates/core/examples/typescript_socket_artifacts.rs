//! Portable process entrypoint for the shared socket artifact compiler.
use anyhow::{Context, Result, bail};
use std::io::{Read as _, Write as _};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args.next().context("socket contract artifact path")?;
    let target = args.next().context("target: typescript, rust or all")?;
    let source = args.next().unwrap_or_else(|| "socket-contract".into());
    if args.next().is_some() {
        bail!("Expected artifact path, target, and optional source");
    }
    let mut bytes = Vec::new();
    std::fs::File::open(input)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let output = forklaunch_core::build::socket_artifact_sources(&bytes, &target, &source)?;
    std::io::stdout().write_all(&serde_json::to_vec(&output)?)?;
    Ok(())
}
