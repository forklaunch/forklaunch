use std::path::Path;

use anyhow::Result;
use serde_json::{json, to_string_pretty};

use crate::{
    constants::Runtime,
    core::{manifest::application::ApplicationManifestData, rendered_template::RenderedTemplate},
};

pub(crate) fn generate_vscode_settings(path_dir: &Path) -> Result<Option<RenderedTemplate>> {
    let path = path_dir.join(".vscode").join("settings.json");
    if path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path,
        content: to_string_pretty(&json!({
          "typescript.experimental.useTsgo": false,
          "editor.codeActionsOnSave": {
            "source.organizeImports": "explicit"
          },
          "editor.formatOnPaste": true,
          "editor.formatOnSave": true,
          "[plaintext]": {
            "editor.formatOnSave": false
          },
          "[markdown]": {
            "editor.formatOnSave": false
          }
        }))?,
        context: None,
    }))
}

pub(crate) fn generate_vscode_tasks(
    path_dir: &Path,
    manifest_data: &ApplicationManifestData,
) -> Result<Option<RenderedTemplate>> {
    let path = path_dir.join(".vscode").join("tasks.json");
    if path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path,
        content: to_string_pretty(&json!({
          "version": "2.0.0",
          "tasks": [
            {
              "label": "Watch Types",
              "type": "shell",
              "command": format!("cd {} && {} run types:watch", manifest_data.modules_path, match manifest_data.runtime.parse()? {
                Runtime::Bun => "bun",
                Runtime::Node => "pnpm",
              }),
              "problemMatcher": "$tsc-watch",
              "group": "build",
              "runOptions": { "runOn": "folderOpen" },
              "presentation": {
                "reveal": "silent",
                "panel": "shared",
                "clear": true,
                "showReuseMessage": false,
                "close": true
              },
              "isBackground": true
            }
          ]
        }))?,
        context: None,
    }))
}
