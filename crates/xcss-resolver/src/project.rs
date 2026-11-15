//! Shared project configuration. Paths are relative to the explicit project root.
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct ProjectConfig {
    pub suffix_length: usize,
    pub tsconfig: PathBuf,
    pub explicit_tsconfig: bool,
    pub document: serde_json::Value,
}
impl ProjectConfig {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("xcss.config.json");
        let parse = || -> Result<Self, String> {
            let document: serde_json::Value = match fs::read(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(e) => return Err(e.to_string()),
            };
            if !document.is_object() {
                return Err("expected an object".into());
            }
            let suffix_length = match document.get("suffixLength") {
                None => 3,
                Some(value) => value
                    .as_u64()
                    .filter(|n| (1..=16).contains(n))
                    .ok_or("suffixLength must be an integer from 1 through 16")?
                    as usize,
            };
            let tsconfig = match document.get("tsconfig") {
                None => "tsconfig.json",
                Some(value) => value
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("tsconfig must be a nonempty string")?,
            };
            Ok(Self {
                suffix_length,
                tsconfig: root.join(tsconfig),
                explicit_tsconfig: document.get("tsconfig").is_some(),
                document,
            })
        };
        parse().map_err(|e| format!("{}: {e}", path.display()))
    }
}
