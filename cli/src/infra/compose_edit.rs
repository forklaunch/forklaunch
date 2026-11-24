//! Writing a changed `docker-compose.yaml` without reformatting it.
//!
//! The commands change the compose file through its model (`DockerCompose`).
//! Serializing the model back is only lossless for a file the CLI wrote and
//! nobody touched: a hand-edited file loses its comments, its anchors and
//! merge keys (`<<: *shared`) and its layout. So [`rewrite`] does one of
//! three things:
//!
//! - the file is exactly what the model serializes to: the new model is
//!   serialized (nothing can be lost);
//! - otherwise the difference between the old and the new model is applied
//!   to the text as line insertions and removals, every other byte kept, and
//!   the result is parsed again to prove it is the intended file;
//! - when a difference cannot be applied that way, the file is left alone
//!   and the caller prints what to change by hand.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde_yml::{Mapping, Value};

use crate::core::docker::DockerCompose;

pub(crate) enum ComposeRewrite {
    Unchanged,
    Edited(String),
    /// The file cannot be edited without reformatting it; `snippet` is the
    /// change to make by hand.
    Manual {
        reason: String,
        snippet: String,
    },
}

/// The file's leading comment block, which serializing the model drops.
fn header(text: &str) -> String {
    text.split_inclusive('\n')
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .collect()
}

/// `original` changed into `after`, the model of `original` with a command's
/// changes applied.
pub(crate) fn rewrite(original: &str, after: &DockerCompose) -> Result<ComposeRewrite> {
    let before: DockerCompose =
        serde_yml::from_str(original).context("parsing docker-compose.yaml")?;
    let before_yaml = serde_yml::to_string(&before)?;
    let after_yaml = serde_yml::to_string(after)?;
    if before_yaml == after_yaml {
        return Ok(ComposeRewrite::Unchanged);
    }
    let header = header(original);
    if format!("{header}{before_yaml}") == original {
        return Ok(ComposeRewrite::Edited(format!("{header}{after_yaml}")));
    }

    let before_value: Value = serde_yml::from_str(&before_yaml)?;
    let after_value: Value = serde_yml::from_str(&after_yaml)?;
    let mut ops = Vec::new();
    diff(&before_value, &after_value, &mut Vec::new(), &mut ops);
    let manual = |reason: String| ComposeRewrite::Manual {
        reason,
        snippet: snippet(&ops),
    };

    let mut lines: Vec<String> = original.split_inclusive('\n').map(str::to_owned).collect();
    let step = indent_step(&lines);
    for op in &ops {
        if let Err(reason) = apply(&mut lines, op, step) {
            return Ok(manual(reason));
        }
    }
    let edited: String = lines.concat();
    // The proof: the edited text is the intended file.
    let intended = comparable(serde_json::to_value(after)?);
    let reparsed = serde_yml::from_str::<DockerCompose>(&edited)
        .ok()
        .and_then(|model| serde_json::to_value(&model).ok())
        .map(comparable);
    if reparsed.as_ref() != Some(&intended) {
        return Ok(manual(
            "its layout could not be edited in place safely".to_owned(),
        ));
    }
    Ok(ComposeRewrite::Edited(edited))
}

/// A model without its merge keys. The model holds `<<: *shared` as the text
/// of what the alias points at, so an entry added to the anchored block
/// shows up there too; the entries themselves are what is compared.
fn comparable(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .filter(|(key, _)| key != "<<")
                .map(|(key, value)| (key, comparable(value)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(comparable).collect())
        }
        other => other,
    }
}

/// The environment variable names `service` sets, merge keys (`<<: *shared`)
/// and list form (`- KEY=value`) included.
pub(crate) fn effective_environment(text: &str, service: &str) -> BTreeSet<String> {
    let Ok(mut value) = serde_yml::from_str::<Value>(text) else {
        return BTreeSet::new();
    };
    // A merged block can itself merge another; each pass resolves a level.
    for _ in 0..8 {
        if value.apply_merge().is_err() || !has_merge_key(&value) {
            break;
        }
    }
    let Some(environment) = value
        .get("services")
        .and_then(|services| services.get(service))
        .and_then(|service| service.get("environment"))
    else {
        return BTreeSet::new();
    };
    match environment {
        Value::Mapping(map) => map
            .keys()
            .filter_map(|key| key.as_str().map(str::to_owned))
            .collect(),
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|item| item.split('=').next().unwrap_or(item).to_owned())
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn has_merge_key(value: &Value) -> bool {
    match value {
        Value::Mapping(map) => map
            .iter()
            .any(|(key, value)| key.as_str() == Some("<<") || has_merge_key(value)),
        Value::Sequence(items) => items.iter().any(has_merge_key),
        _ => false,
    }
}

// ------------------------------------------------------------------ the diff

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Add {
        path: Vec<String>,
        key: String,
        value: Value,
    },
    Remove {
        path: Vec<String>,
        key: String,
    },
    Replace {
        path: Vec<String>,
        key: String,
        value: Value,
    },
}

fn diff(before: &Value, after: &Value, path: &mut Vec<String>, ops: &mut Vec<Op>) {
    let (Value::Mapping(before), Value::Mapping(after)) = (before, after) else {
        return;
    };
    // A merge key is never the model's to change (see `comparable`).
    for (key, old) in before {
        let Some(name) = key.as_str().filter(|name| *name != "<<") else {
            continue;
        };
        match after.get(key) {
            None => ops.push(Op::Remove {
                path: path.clone(),
                key: name.to_owned(),
            }),
            Some(new) if new == old => {}
            Some(new) if old.is_mapping() && new.is_mapping() => {
                path.push(name.to_owned());
                diff(old, new, path, ops);
                path.pop();
            }
            Some(new) => ops.push(Op::Replace {
                path: path.clone(),
                key: name.to_owned(),
                value: new.clone(),
            }),
        }
    }
    for (key, new) in after {
        let Some(name) = key.as_str().filter(|name| *name != "<<") else {
            continue;
        };
        if !before.contains_key(key) {
            ops.push(Op::Add {
                path: path.clone(),
                key: name.to_owned(),
                value: new.clone(),
            });
        }
    }
}

/// What to change by hand, as YAML to merge in plus the keys to take out.
fn snippet(ops: &[Op]) -> String {
    let mut root = Mapping::new();
    let mut removed = Vec::new();
    for op in ops {
        match op {
            Op::Add { path, key, value } | Op::Replace { path, key, value } => {
                let mut at = &mut root;
                for segment in path {
                    let entry = at
                        .entry(Value::String(segment.clone()))
                        .or_insert_with(|| Value::Mapping(Mapping::new()));
                    if !entry.is_mapping() {
                        *entry = Value::Mapping(Mapping::new());
                    }
                    at = entry.as_mapping_mut().expect("just made a mapping");
                }
                at.insert(Value::String(key.clone()), value.clone());
            }
            Op::Remove { path, key } => {
                removed.push(format!("{}.{key}", path.join(".")));
            }
        }
    }
    let mut out = String::new();
    if !root.is_empty() {
        out.push_str(&serde_yml::to_string(&root).unwrap_or_default());
    }
    for key in removed {
        out.push_str(&format!("# remove: {}\n", key.trim_start_matches('.')));
    }
    out
}

// -------------------------------------------------------------- text editing

/// The indentation of a line that holds content; `None` for a blank or
/// comment-only line.
fn content_indent(line: &str) -> Option<usize> {
    let trimmed = line.trim_start_matches(' ');
    let body = trimmed.trim_end();
    if body.is_empty() || body.starts_with('#') {
        return None;
    }
    Some(line.len() - trimmed.len())
}

fn is_sequence_item(line: &str) -> bool {
    let body = line.trim();
    body == "-" || body.starts_with("- ")
}

/// A mapping line's key and what follows the colon (comment removed when the
/// rest is nothing else).
fn split_key(line: &str) -> Option<(String, &str)> {
    let body = line.trim();
    if body.starts_with('#') || is_sequence_item(line) || body.starts_with('?') {
        return None;
    }
    let (key, rest) = match body.chars().next()? {
        quote @ ('"' | '\'') => {
            let close = body[1..].find(quote)? + 1;
            let rest = body[close + 1..].strip_prefix(':')?;
            (body[1..close].to_owned(), rest)
        }
        _ => {
            let colon = body
                .match_indices(':')
                .map(|(index, _)| index)
                .find(|index| {
                    body[index + 1..]
                        .chars()
                        .next()
                        .is_none_or(char::is_whitespace)
                })?;
            (body[..colon].trim_end().to_owned(), &body[colon + 1..])
        }
    };
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim();
    Some((key, if rest.starts_with('#') { "" } else { rest }))
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Kind {
    /// Children on the following lines (or nothing).
    Block,
    /// `{}`.
    EmptyFlow,
    Flow,
    BlockScalar,
    Alias,
    Inline,
}

fn kind(rest: &str) -> Kind {
    let rest = rest.trim();
    match rest.chars().next() {
        None | Some('#') => Kind::Block,
        // An anchor or a tag comes before the value it marks.
        Some('&' | '!') => kind(rest.split_once(char::is_whitespace).map_or("", |(_, r)| r)),
        Some('|' | '>') => Kind::BlockScalar,
        Some('{') if rest.replace(' ', "").starts_with("{}") => Kind::EmptyFlow,
        Some('{' | '[') => Kind::Flow,
        Some('*') => Kind::Alias,
        Some(_) => Kind::Inline,
    }
}

/// One past the last content line that belongs to the key at `key_line`
/// (indented `indent`), looking no further than `limit`. Trailing blank and
/// comment lines are left to whatever follows.
fn block_end(lines: &[String], key_line: usize, indent: usize, limit: usize) -> usize {
    let mut end = key_line + 1;
    for (index, line) in lines.iter().enumerate().take(limit).skip(key_line + 1) {
        match content_indent(line) {
            None => {}
            Some(n) if n > indent || (n == indent && is_sequence_item(line)) => end = index + 1,
            Some(_) => break,
        }
    }
    end
}

#[derive(Debug)]
struct Located {
    /// The key's own line; `None` for the document root.
    key_line: Option<usize>,
    indent: Option<usize>,
    /// The key's children: `start..end`.
    start: usize,
    end: usize,
    kind: Kind,
}

fn first_content(lines: &[String], start: usize, end: usize) -> Option<usize> {
    (start..end).find(|index| content_indent(&lines[*index]).is_some())
}

/// The line of `key` among the mapping entries in `start..end`.
fn find_child(lines: &[String], start: usize, end: usize, key: &str) -> Option<(usize, usize)> {
    let first = first_content(lines, start, end)?;
    if is_sequence_item(&lines[first]) {
        return None;
    }
    let indent = content_indent(&lines[first])?;
    (start..end)
        .find(|index| {
            content_indent(&lines[*index]) == Some(indent)
                && split_key(&lines[*index]).is_some_and(|(name, _)| name == key)
        })
        .map(|index| (index, indent))
}

fn locate(lines: &[String], path: &[String]) -> std::result::Result<Located, String> {
    let mut at = Located {
        key_line: None,
        indent: None,
        start: 0,
        end: lines.len(),
        kind: Kind::Block,
    };
    for (depth, key) in path.iter().enumerate() {
        let name = path[..=depth].join(".");
        if at.kind != Kind::Block {
            return Err(format!(
                "`{}` is not a block mapping",
                path[..depth].join(".")
            ));
        }
        let (line, indent) = find_child(lines, at.start, at.end, key)
            .ok_or_else(|| format!("`{name}` was not found as a block mapping key"))?;
        let rest = split_key(&lines[line]).map_or("", |(_, rest)| rest);
        at = Located {
            key_line: Some(line),
            indent: Some(indent),
            start: line + 1,
            end: block_end(lines, line, indent, at.end),
            kind: kind(rest),
        };
    }
    Ok(at)
}

/// The indentation the file adds per level.
fn indent_step(lines: &[String]) -> usize {
    lines
        .iter()
        .filter_map(|line| content_indent(line))
        .filter(|indent| *indent > 0)
        .min()
        .unwrap_or(2)
}

fn scalar(value: &Value) -> std::result::Result<String, String> {
    if !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_)) {
        return Err("a changed value is not a single scalar".to_owned());
    }
    let text = serde_yml::to_string(value).map_err(|error| error.to_string())?;
    let text = text.trim_end();
    if text.contains('\n') {
        return Err("a changed value spans several lines".to_owned());
    }
    Ok(text.to_owned())
}

/// `key: value` as block YAML, every line indented `indent` spaces.
fn render_entry(
    key: &str,
    value: &Value,
    indent: usize,
) -> std::result::Result<Vec<String>, String> {
    let mut entry = Mapping::new();
    entry.insert(Value::String(key.to_owned()), value.clone());
    let text = serde_yml::to_string(&entry).map_err(|error| error.to_string())?;
    let pad = " ".repeat(indent);
    Ok(text
        .lines()
        .map(|line| {
            if line.is_empty() {
                "\n".to_owned()
            } else {
                format!("{pad}{line}\n")
            }
        })
        .collect())
}

fn end_with_newline(lines: &mut [String]) {
    if let Some(last) = lines.last_mut()
        && !last.ends_with('\n')
    {
        last.push('\n');
    }
}

/// Whether the entries of the mapping in `start..end` (indented `indent`)
/// are set apart by blank lines.
fn blank_line_separated(lines: &[String], start: usize, end: usize, indent: usize) -> bool {
    let keys: Vec<usize> = (start..end)
        .filter(|index| {
            content_indent(&lines[*index]) == Some(indent) && split_key(&lines[*index]).is_some()
        })
        .collect();
    keys.len() > 1
        && keys[1..].iter().all(|index| {
            (start..*index)
                .rev()
                .take_while(|above| content_indent(&lines[*above]).is_none())
                .any(|above| lines[above].trim().is_empty())
        })
}

fn apply(lines: &mut Vec<String>, op: &Op, step: usize) -> std::result::Result<(), String> {
    match op {
        Op::Add { path, key, value } => add(lines, path, key, value, step),
        Op::Remove { path, key } => remove(lines, path, key),
        Op::Replace { path, key, value } => replace(lines, path, key, value),
    }
}

fn add(
    lines: &mut Vec<String>,
    path: &[String],
    key: &str,
    value: &Value,
    step: usize,
) -> std::result::Result<(), String> {
    // A top-level section the file does not have yet goes at the end.
    if let [section] = path
        && find_child(lines, 0, lines.len(), section).is_none()
    {
        end_with_newline(lines);
        if lines.last().is_some_and(|line| !line.trim().is_empty()) {
            lines.push("\n".to_owned());
        }
        lines.push(format!("{section}:\n"));
        lines.extend(render_entry(key, value, step)?);
        return Ok(());
    }
    let mut at = locate(lines, path)?;
    if at.kind == Kind::EmptyFlow {
        // `volumes: {}` becomes a block to hold the entry.
        let line = at.key_line.expect("an empty mapping has a key line");
        let text = &lines[line];
        let open = text.find('{').expect("an empty flow mapping");
        if text[open..].replace(' ', "").trim_end() != "{}" {
            return Err(format!(
                "`{}` could not be opened as a block",
                path.join(".")
            ));
        }
        lines[line] = format!("{}\n", text[..open].trim_end());
        at.kind = Kind::Block;
        at.end = at.start;
    }
    if at.kind != Kind::Block {
        return Err(format!("`{}` is not a block mapping", path.join(".")));
    }
    end_with_newline(lines);
    let first = first_content(lines, at.start, at.end);
    let rendered = match first {
        Some(first) if is_sequence_item(&lines[first]) => {
            // `environment:` written as a list of KEY=value.
            if path.last().map(String::as_str) != Some("environment") {
                return Err(format!("`{}` is a list", path.join(".")));
            }
            let Value::String(text) = value else {
                return Err(format!("`{}` is a list", path.join(".")));
            };
            let indent = content_indent(&lines[first]).unwrap_or(0);
            let item = scalar(&Value::String(format!("{key}={text}")))?;
            vec![format!("{}- {item}\n", " ".repeat(indent))]
        }
        Some(first) => {
            let indent = content_indent(&lines[first]).unwrap_or(0);
            let mut rendered = render_entry(key, value, indent)?;
            if value.is_mapping() && blank_line_separated(lines, at.start, at.end, indent) {
                rendered.insert(0, "\n".to_owned());
            }
            rendered
        }
        None => render_entry(key, value, at.indent.map_or(0, |indent| indent + step))?,
    };
    let position = if first.is_some() { at.end } else { at.start };
    lines.splice(position..position, rendered);
    Ok(())
}

fn remove(lines: &mut Vec<String>, path: &[String], key: &str) -> std::result::Result<(), String> {
    let at = locate(lines, path)?;
    if at.kind != Kind::Block {
        return Err(format!("`{}` is not a block mapping", path.join(".")));
    }
    let name = format!("{}.{key}", path.join("."));
    let first =
        first_content(lines, at.start, at.end).ok_or_else(|| format!("`{name}` was not found"))?;
    let (from, to) = if is_sequence_item(&lines[first]) {
        if path.last().map(String::as_str) != Some("environment") {
            return Err(format!("`{}` is a list", path.join(".")));
        }
        let indent = content_indent(&lines[first]);
        let line = (at.start..at.end)
            .find(|index| {
                content_indent(&lines[*index]) == indent && {
                    let item = lines[*index].trim().trim_start_matches('-').trim();
                    let item = item.trim_matches(|c| c == '"' || c == '\'');
                    item == key || item.starts_with(&format!("{key}="))
                }
            })
            .ok_or_else(|| format!("`{name}` was not found"))?;
        let next = block_end(lines, line, indent.unwrap_or(0) + 1, at.end);
        if next != line + 1 {
            return Err(format!("`{name}` spans several lines"));
        }
        (line, line + 1)
    } else {
        let (line, indent) = find_child(lines, at.start, at.end, key)
            .ok_or_else(|| format!("`{name}` was not found as a block mapping key"))?;
        let rest = split_key(&lines[line]).map_or("", |(_, rest)| rest);
        if kind(rest) == Kind::Flow && !rest.trim_end().ends_with(['}', ']']) {
            return Err(format!("`{name}` is a flow value over several lines"));
        }
        (line, block_end(lines, line, indent, at.end))
    };
    // A blank line that set the entry apart goes with it when the entry was
    // the last of its block.
    let last_of_block = to >= at.end;
    let from = if last_of_block
        && from > at.start
        && lines[from - 1].trim().is_empty()
        && lines.get(to).is_none_or(|line| line.trim().is_empty())
    {
        from - 1
    } else {
        from
    };
    lines.drain(from..to);
    // An emptied mapping is written `{}`: a bare key would read as null.
    if let Some(parent) = at.key_line {
        let end = block_end(lines, parent, at.indent.unwrap_or(0), lines.len());
        if first_content(lines, parent + 1, end).is_none() {
            let text = lines[parent].trim_end().to_owned();
            if !text.ends_with(':') {
                return Err(format!("`{}` would be left empty", path.join(".")));
            }
            lines[parent] = format!("{text} {{}}\n");
        }
    }
    Ok(())
}

fn replace(
    lines: &mut [String],
    path: &[String],
    key: &str,
    value: &Value,
) -> std::result::Result<(), String> {
    let name = format!("{}.{key}", path.join("."));
    let at = locate(lines, path)?;
    if at.kind != Kind::Block {
        return Err(format!("`{}` is not a block mapping", path.join(".")));
    }
    let (line, indent) = find_child(lines, at.start, at.end, key)
        .ok_or_else(|| format!("`{name}` was not found as a block mapping key"))?;
    let text = lines[line].clone();
    let rest = split_key(&text).map_or("", |(_, rest)| rest);
    if kind(rest) != Kind::Inline
        || block_end(lines, line, indent, at.end) != line + 1
        || rest.contains(" #")
        || !text.trim_end().ends_with(rest)
    {
        return Err(format!(
            "`{name}` changes, and its value is not a plain one-line scalar"
        ));
    }
    let rendered = scalar(value)?;
    let kept = text.trim_end();
    let kept = &kept[..kept.len() - rest.len()];
    lines[line] = format!("{kept}{rendered}\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ComposeRewrite, effective_environment, rewrite};
    use crate::core::docker::DockerCompose;

    const CUSTOM: &str = r#"# Local stack. Pins are raised on purpose, one image at a time.

x-shared: &shared-environment
  LOG_LEVEL: debug # chatty on purpose
  S3_REGION: us-west-2

volumes:
  # Kept between runs.
  pg-data:

networks:
  demo-network:
    name: demo-network
    driver: bridge

services:
  # The HTTP API.
  api:
    image: demo-api:local
    networks:
      - demo-network
    environment: &api-environment
      <<: *shared-environment
      PORT: "8000" # the port the load balancer expects
      DB_HOST: postgresql

  jobs-worker:
    image: demo-jobs:local
    networks: [demo-network]
    environment:
      <<: *api-environment
      QUEUE_NAME: jobs
    command: |
      node worker.js
      # not a comment: part of the script

  listed:
    image: demo-listed:local
    environment:
      - PORT=8001
      - DB_HOST=postgresql
"#;

    fn model(text: &str) -> DockerCompose {
        serde_yml::from_str(text).unwrap()
    }

    fn edited(original: &str, after: &DockerCompose) -> String {
        match rewrite(original, after).unwrap() {
            ComposeRewrite::Edited(text) => text,
            ComposeRewrite::Unchanged => panic!("unchanged"),
            ComposeRewrite::Manual { reason, snippet } => panic!("manual: {reason}\n{snippet}"),
        }
    }

    /// Lines of `before` that `after` no longer has.
    fn lost(before: &str, after: &str) -> Vec<String> {
        let mut remaining: Vec<&str> = after.lines().collect();
        before
            .lines()
            .filter(|line| match remaining.iter().position(|l| l == line) {
                Some(index) => {
                    remaining.remove(index);
                    false
                }
                None => true,
            })
            .map(str::to_owned)
            .collect()
    }

    fn set_env(compose: &mut DockerCompose, service: &str, pairs: &[(&str, &str)]) {
        let environment = compose
            .services
            .get_mut(service)
            .unwrap()
            .environment
            .get_or_insert_with(Default::default);
        for (key, value) in pairs {
            environment.insert((*key).to_owned(), (*value).to_owned());
        }
    }

    #[test]
    fn an_untouched_generated_file_is_serialized_as_before() {
        let generated = serde_yml::to_string(&model(CUSTOM)).unwrap();
        let original = format!("# Generated by ForkLaunch\n\n{generated}");
        let mut after = model(&original);
        set_env(&mut after, "api", &[("S3_BUCKET", "demo-api-dev")]);
        let text = edited(&original, &after);
        assert_eq!(
            text,
            format!(
                "# Generated by ForkLaunch\n\n{}",
                serde_yml::to_string(&after).unwrap()
            )
        );
    }

    #[test]
    fn a_customized_file_keeps_comments_anchors_merge_keys_and_order() {
        let mut after = model(CUSTOM);
        set_env(
            &mut after,
            "api",
            &[
                ("S3_URL", "http://minio:9000"),
                ("S3_BUCKET", "demo-api-dev"),
            ],
        );
        set_env(&mut after, "jobs-worker", &[("S3_BUCKET", "demo-jobs-dev")]);
        set_env(&mut after, "listed", &[("S3_BUCKET", "demo-listed-dev")]);
        crate::core::docker::add_s3_to_docker_compose(
            "demo",
            "api",
            &mut after,
            &mut Default::default(),
        )
        .unwrap();
        let text = edited(CUSTOM, &after);

        assert_eq!(lost(CUSTOM, &text), Vec::<String>::new(), "{text}");
        for kept in [
            "x-shared: &shared-environment",
            "      <<: *shared-environment",
            "      <<: *api-environment",
            "      PORT: \"8000\" # the port the load balancer expects",
            "  # The HTTP API.",
            "      # not a comment: part of the script",
        ] {
            assert!(text.lines().any(|line| line == kept), "{kept}\n{text}");
        }
        // The new settings sit at the end of each service's own block.
        assert!(text.contains("      DB_HOST: postgresql\n      S3_URL: http://minio:9000\n      S3_BUCKET: demo-api-dev\n"), "{text}");
        assert!(
            text.contains(
                "      QUEUE_NAME: jobs\n      S3_BUCKET: demo-jobs-dev\n    command: |\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("      - DB_HOST=postgresql\n      - S3_BUCKET=demo-listed-dev\n"),
            "{text}"
        );
        // MinIO and its volume are appended to their sections.
        assert!(
            text.contains("  pg-data:\n  minio-data:\n    driver: local\n"),
            "{text}"
        );
        assert!(
            text.contains("\n\n  minio:\n    container_name: demo-minio\n"),
            "{text}"
        );
        // Merge keys still resolve: the worker inherits what the API sets.
        let inherited = effective_environment(&text, "jobs-worker");
        for key in ["LOG_LEVEL", "PORT", "S3_URL", "QUEUE_NAME"] {
            assert!(inherited.contains(key), "{key} in {inherited:?}");
        }
        serde_yml::from_str::<serde_yml::Value>(&text).unwrap();
    }

    #[test]
    fn removing_what_was_added_restores_every_byte() {
        let mut added = model(CUSTOM);
        set_env(&mut added, "api", &[("S3_BUCKET", "demo-api-dev")]);
        set_env(&mut added, "listed", &[("S3_BUCKET", "demo-listed-dev")]);
        crate::core::docker::add_s3_to_docker_compose(
            "demo",
            "api",
            &mut added,
            &mut Default::default(),
        )
        .unwrap();
        let text = edited(CUSTOM, &added);
        assert_ne!(text, CUSTOM);
        assert_eq!(edited(&text, &model(CUSTOM)), CUSTOM);
    }

    #[test]
    fn a_change_that_cannot_be_placed_leaves_the_file_and_gives_the_snippet() {
        // The service's environment is an alias: there is no block to add to.
        let original = "services:\n  api:\n    image: demo # pinned\n    environment: *shared\n";
        let original = format!("x-shared: &shared\n  A: b\n{original}");
        let mut after = model(&original);
        set_env(&mut after, "api", &[("S3_BUCKET", "demo-api-dev")]);
        match rewrite(&original, &after).unwrap() {
            ComposeRewrite::Manual { reason, snippet } => {
                assert!(reason.contains("services.api.environment"), "{reason}");
                assert!(
                    snippet.contains(
                        "services:\n  api:\n    environment:\n      S3_BUCKET: demo-api-dev\n"
                    ),
                    "{snippet}"
                );
            }
            _ => panic!("expected a manual change"),
        }

        // A changed multi-line value is not rewritten either.
        let original = "# mine\nservices:\n  api:\n    image: demo\n    command: >\n      node\n      server.js\n";
        let mut after = model(original);
        after.services.get_mut("api").unwrap().command =
            Some(crate::core::docker::Command::Simple("node other.js".into()));
        assert!(matches!(
            rewrite(original, &after).unwrap(),
            ComposeRewrite::Manual { .. }
        ));
    }

    #[test]
    fn a_one_line_value_is_replaced_in_place() {
        let original = "# mine\nservices:\n  init:\n    image: minio\n    entrypoint: /bin/sh -c \"mc mb local/a\"\n    restart: 'no'\n";
        let mut after = model(original);
        after.services.get_mut("init").unwrap().entrypoint = Some(
            crate::core::docker::Command::Simple("/bin/sh -c \"mc mb local/a local/b\"".into()),
        );
        let text = edited(original, &after);
        assert!(text.starts_with("# mine\nservices:\n  init:\n    image: minio\n    entrypoint: "));
        assert!(text.ends_with("    restart: 'no'\n"));
        assert!(text.contains("local/a local/b"));
    }

    #[test]
    fn nothing_to_change_is_reported_as_unchanged() {
        assert!(matches!(
            rewrite(CUSTOM, &model(CUSTOM)).unwrap(),
            ComposeRewrite::Unchanged
        ));
    }

    #[test]
    fn effective_environment_follows_merge_keys_and_lists() {
        let api = effective_environment(CUSTOM, "api");
        assert!(
            api.contains("S3_REGION") && api.contains("DB_HOST"),
            "{api:?}"
        );
        let worker = effective_environment(CUSTOM, "jobs-worker");
        assert!(worker.contains("S3_REGION") && worker.contains("QUEUE_NAME"));
        let listed = effective_environment(CUSTOM, "listed");
        assert!(listed.contains("PORT") && listed.contains("DB_HOST"));
        assert!(effective_environment(CUSTOM, "nobody").is_empty());
    }
}
