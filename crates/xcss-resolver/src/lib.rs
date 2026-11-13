//! Filesystem adapter shared by the CLI and Node host. Never used by the language engine.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use xcss_core::{FileId, GenerateOptions};
#[derive(Clone, Default)]
pub struct Resolver {
    pub configs: BTreeSet<PathBuf>,
    paths: BTreeMap<String, Vec<PathBuf>>,
}
#[derive(Default)]
struct Config {
    base: Option<PathBuf>,
    paths: Option<(PathBuf, BTreeMap<String, Vec<String>>)>,
}
impl Resolver {
    pub fn load(root: &Path, config: Option<&Path>) -> Result<Self, String> {
        let path = config
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.join("tsconfig.json"));
        if config.is_none() && !path.exists() {
            return Ok(Self::default());
        }
        let mut resolver = Self::default();
        let loaded = load_config(&path, &mut resolver.configs, &mut BTreeSet::new())?;
        if let Some((directory, paths)) = loaded.paths {
            let base = loaded.base.unwrap_or(directory);
            for (key, targets) in paths {
                if key.matches('*').count() > 1
                    || targets.iter().any(|t| t.matches('*').count() > 1)
                {
                    return Err(format!("Invalid paths pattern: {key}"));
                }
                resolver
                    .paths
                    .insert(key, targets.into_iter().map(|t| base.join(t)).collect());
            }
        }
        Ok(resolver)
    }
    /// None means no alias matched; a matched but missing target is an error.
    pub fn alias(&self, specifier: &str) -> Result<Option<PathBuf>, String> {
        if specifier.starts_with(['.', '/', '#', '?']) || specifier.contains(':') {
            return Ok(None);
        }
        let mut matches = self
            .paths
            .iter()
            .filter_map(|(key, targets)| {
                if key == specifier {
                    return Some((usize::MAX, key.len(), targets, ""));
                }
                let (prefix, suffix) = key.split_once('*')?;
                let middle = specifier.strip_prefix(prefix)?.strip_suffix(suffix)?;
                Some((prefix.len(), key.len(), targets, middle))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        let Some((_, _, targets, middle)) = matches.first() else {
            return Ok(None);
        };
        for target in *targets {
            let path = PathBuf::from(target.to_string_lossy().replace('*', middle));
            if path.is_file() {
                return canonical(&path).map(Some).map_err(|e| e.to_string());
            }
        }
        Err(format!("Cannot resolve tsconfig paths alias '{specifier}'"))
    }
    pub fn import(&self, importer: &Path, specifier: &str) -> Result<Option<PathBuf>, String> {
        if let Some(path) = self.alias(specifier)? {
            return Ok(Some(path));
        }
        // Bare stylesheet paths retain their existing file-relative meaning.
        Ok(canonical(importer.parent().unwrap().join(specifier))
            .ok()
            .filter(|p| p.is_file()))
    }
    pub fn assets(
        &self,
        root: &Path,
        id: &FileId,
        source: &str,
        options: &mut GenerateOptions,
    ) -> Result<(), String> {
        for asset in xcss_core::inspect_project_file(source).assets {
            let split = asset.url.find(['?', '#']).unwrap_or(asset.url.len());
            let (path, suffix) = asset.url.split_at(split);
            let path = decode_percent(path);
            match self.alias(&path) {
                Ok(Some(target)) => {
                    let target = pathdiff::diff_paths(target, root)
                        .ok_or("Asset is on a different filesystem")?;
                    options
                        .asset_resolutions
                        .entry(id.clone())
                        .or_default()
                        .insert(
                            asset.url.clone(),
                            Ok(format!(
                                "{}{suffix}",
                                encode_path(&target.to_string_lossy().replace('\\', "/"))
                            )),
                        );
                }
                Err(error) => {
                    options
                        .asset_resolutions
                        .entry(id.clone())
                        .or_default()
                        .insert(asset.url, Err(error));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
fn decode_percent(path: &str) -> String {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.into())
}
fn load_config(
    path: &Path,
    seen: &mut BTreeSet<PathBuf>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<Config, String> {
    let path = canonical(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !active.insert(path.clone()) {
        return Err(format!("Circular tsconfig extends: {}", path.display()));
    }
    seen.insert(path.clone());
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&jsonc(&text)?).map_err(|e| format!("{}: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{}: expected configuration object", path.display()));
    }
    let directory = path.parent().unwrap();
    let mut config = Config::default();
    if let Some(extends) = value.get("extends") {
        let entries = if let Some(s) = extends.as_str() {
            vec![s]
        } else {
            extends
                .as_array()
                .ok_or("extends must be a string or array")?
                .iter()
                .map(|v| v.as_str().ok_or("extends entries must be strings"))
                .collect::<Result<Vec<_>, _>>()?
        };
        for entry in entries {
            let parent = load_config(&extends_path(directory, entry)?, seen, active)?;
            if parent.base.is_some() {
                config.base = parent.base;
            }
            if parent.paths.is_some() {
                config.paths = parent.paths;
            }
        }
    }
    if let Some(options) = value.get("compilerOptions") {
        if !options.is_object() {
            return Err("compilerOptions must be an object".into());
        }
        if let Some(base) = options.get("baseUrl") {
            config.base = Some(directory.join(base.as_str().ok_or("baseUrl must be a string")?));
        }
        if let Some(paths) = options.get("paths") {
            let mut parsed = BTreeMap::new();
            for (key, targets) in paths.as_object().ok_or("paths must be an object")? {
                let targets = targets
                    .as_array()
                    .ok_or("paths targets must be arrays")?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(str::to_owned)
                            .ok_or("paths targets must be strings")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                parsed.insert(key.clone(), targets);
            }
            config.paths = Some((directory.to_path_buf(), parsed));
        }
    }
    active.remove(&path);
    Ok(config)
}
fn extends_path(directory: &Path, entry: &str) -> Result<PathBuf, String> {
    fn candidate(path: PathBuf) -> Option<PathBuf> {
        if path.is_file() {
            return Some(path);
        }
        let json = PathBuf::from(format!("{}.json", path.display()));
        if json.is_file() {
            return Some(json);
        }
        if path.is_dir() {
            if let Ok(text) = fs::read_to_string(path.join("package.json")) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(config) = value.get("tsconfig").and_then(|v| v.as_str()) {
                        let target = path.join(config);
                        if target.is_file() {
                            return Some(target);
                        }
                    }
                }
            }
            let config = path.join("tsconfig.json");
            if config.is_file() {
                return Some(config);
            }
        }
        None
    }
    if entry.starts_with('.') || Path::new(entry).is_absolute() {
        return candidate(directory.join(entry))
            .ok_or_else(|| format!("Cannot resolve extended tsconfig '{entry}'"));
    }
    for parent in directory.ancestors() {
        if let Some(path) = candidate(parent.join("node_modules").join(entry)) {
            return Ok(path);
        }
    }
    Err(format!("Cannot resolve extended tsconfig '{entry}'"))
}
// Strip comments and trailing commas while preserving strings and line positions.
fn jsonc(text: &str) -> Result<String, String> {
    let mut bytes = text.trim_start_matches('\u{feff}').as_bytes().to_vec();
    let mut i = 0;
    let mut string = false;
    while i < bytes.len() {
        if string {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                string = false;
            }
        } else if bytes[i] == b'"' {
            string = true;
        } else if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                bytes[i] = b' ';
                i += 1;
            }
            continue;
        } else if bytes[i..].starts_with(b"/*") {
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                if bytes[i] != b'\n' && bytes[i] != b'\r' {
                    bytes[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("Unterminated tsconfig comment".into());
            }
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            continue;
        }
        i += 1;
    }
    string = false;
    i = 0;
    while i < bytes.len() {
        if string {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                string = false;
            }
        } else if bytes[i] == b'"' {
            string = true;
        } else if bytes[i] == b',' {
            let next = bytes[i + 1..].iter().find(|b| !b.is_ascii_whitespace());
            if next.is_some_and(|b| matches!(b, b'}' | b']')) {
                bytes[i] = b' ';
            }
        }
        i += 1;
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn canonical(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|path| {
        let text = path.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix("//?/UNC/") {
            PathBuf::from(format!("//{rest}"))
        } else {
            PathBuf::from(text.strip_prefix("//?/").unwrap_or(&text))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    fn root() -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/resolver-tests/{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        canonical(root).unwrap()
    }
    #[test]
    fn config_inheritance_patterns_fallbacks_and_jsonc() {
        let root = root();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(root.join("config/icon.svg"), "svg").unwrap();
        fs::write(root.join("config/base.json"), r#"{ // comment
            "compilerOptions": { "paths": { "@asset/*": ["missing/*", "./*"], "exact": ["icon.svg"], }, },
        }"#).unwrap();
        fs::write(root.join("tsconfig.json"), r#"{"extends":"./config/base"}"#).unwrap();
        let resolver = Resolver::load(&root, None).unwrap();
        assert_eq!(resolver.configs.len(), 2);
        assert_eq!(
            resolver.alias("exact").unwrap(),
            Some(root.join("config/icon.svg"))
        );
        assert_eq!(
            resolver.alias("@asset/icon.svg").unwrap(),
            resolver.alias("exact").unwrap()
        );
        assert!(resolver.alias("@asset/missing.svg").is_err());
        assert!(resolver.alias("@/icon.svg").unwrap().is_none());
        assert!(resolver.alias("https://example.com/a").unwrap().is_none());
    }
    #[test]
    fn exact_and_longest_prefix_win_and_base_url_is_not_an_alias() {
        let root = root();
        fs::write(root.join("a"), "a").unwrap();
        fs::write(root.join("b"), "b").unwrap();
        fs::write(root.join("tsconfig.json"), r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["a"],"@/long/*":["b"],"@/long/exact":["a"]}}}"#).unwrap();
        let resolver = Resolver::load(&root, None).unwrap();
        assert_eq!(
            resolver.alias("@/long/exact").unwrap(),
            Some(root.join("a"))
        );
        assert_eq!(
            resolver.alias("@/long/other").unwrap(),
            Some(root.join("b"))
        );
        assert!(resolver.alias("a").unwrap().is_none());
    }
    #[test]
    fn missing_invalid_and_circular_configs_fail() {
        let root = root();
        assert!(Resolver::load(&root, None).is_ok());
        assert!(Resolver::load(&root, Some(&root.join("missing.json"))).is_err());
        fs::write(root.join("tsconfig.json"), "{invalid").unwrap();
        assert!(Resolver::load(&root, None).is_err());
        fs::write(
            root.join("tsconfig.json"),
            r#"{"extends":"./tsconfig.json"}"#,
        )
        .unwrap();
        assert!(
            Resolver::load(&root, None)
                .err()
                .unwrap()
                .contains("Circular")
        );
    }
    #[test]
    fn urls_are_found_without_strings_comments_or_raw_contents() {
        let source = r#"/* url(@a/comment) */ @raw { .raw { background: url(@a/raw); } }
        .a { content: "url(@a/string)"; background: url('@a/icon.svg?q#f'); mask: url(@a/other.svg); }"#;
        let urls = xcss_core::inspect_project_file(source)
            .assets
            .into_iter()
            .map(|a| a.url)
            .collect::<Vec<_>>();
        assert_eq!(urls, vec!["@a/icon.svg?q#f", "@a/other.svg"]);
    }
}

fn encode_path(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    #[test]
    fn percent_decoding_is_safe_for_unicode_and_css_inspection_decodes_escapes() {
        assert_eq!(decode_percent("%?"), "%?");
        assert_eq!(decode_percent("icon%20a.svg"), "icon a.svg");
        let source = r#".a { background: url("@assets/icon\20 a.svg?x#y"); }"#;
        assert_eq!(
            xcss_core::inspect_project_file(source).assets[0].url,
            "@assets/icon a.svg?x#y"
        );
    }
}
