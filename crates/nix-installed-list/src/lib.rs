use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Root {
    pub elements: Elements,
    pub version: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Elements {
    #[serde(flatten)]
    pub packages: HashMap<String, Package>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub active: bool,
    pub attr_path: String,
    pub original_url: String,
    pub outputs: Option<Vec<String>>,
    pub priority: i64,
    pub store_paths: Vec<String>,
    pub url: String,
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },
    #[error("Serde JSON error: {source}")]
    SerdeJson {
        #[from]
        source: serde_json::Error,
    },
}
pub fn nix() -> std::process::Command {
    std::process::Command::new("/nix/var/nix/profiles/default/bin/nix")
}

pub fn manifest() -> Result<String, ProfileError> {
    let mut nix = nix();
    let cmd = nix.arg("profile").arg("list").arg("--json");

    let output = cmd.output()?;

    let stdout = output.stdout;

    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

pub fn manifest_parsed() -> Result<Root, ProfileError> {
    let output = manifest()?;
    let root: Root = serde_json::from_str(&output)?;
    Ok(root)
}

pub fn get_name(package: &Package) -> serde_json::Value {
    let mut nix = nix();
    let cmd = nix
        .arg("eval")
        .arg("--raw")
        .arg(format!("{}#{}.name", package.url, package.attr_path))
        .arg("--apply")
        .arg("builtins.toJSON");

    let output = cmd.output().unwrap();

    let stdout = output.stdout;

    let meta: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    meta
}

pub fn get_version(package: &Package) -> Option<String> {
    let mut nix = nix();
    let cmd = nix
        .arg("eval")
        .arg("--raw")
        .arg(format!("{}#{}.version", package.url, package.attr_path))
        .arg("--apply")
        .arg("builtins.toJSON");

    let output = cmd.output().unwrap();

    let stdout = output.stdout;

    if stdout.len() == 0 {
        return None;
    }

    let meta: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    match meta {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(_) => None,
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Array(_) => None,
        serde_json::Value::Object(_) => None,
    }
}

pub fn get_meta(package: &Package) -> serde_json::Value {
    let mut nix = nix();
    let cmd = nix
        .arg("eval")
        .arg("--raw")
        .arg(format!("{}#{}.meta", package.url, package.attr_path))
        .arg("--apply")
        .arg("builtins.toJSON");

    let output = cmd.output().unwrap();

    let stdout = output.stdout;

    if stdout.len() == 0 {
        return serde_json::Value::Null;
    }

    let meta: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    return meta;
}

#[cfg(test)]
mod test {
    use crate::{get_meta, get_version, manifest, manifest_parsed};

    #[test]
    pub fn test_manifest() {
        let output = manifest().unwrap();
        println!("{}", output);
    }

    #[test]
    pub fn test_manifest_parsed() {
        let output = manifest_parsed().unwrap();
        println!("{:?}", output);
    }
}
