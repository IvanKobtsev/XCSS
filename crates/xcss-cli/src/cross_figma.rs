use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    output: String,
    #[serde(default)]
    screens: BTreeMap<String, Screen>,
    #[serde(default)]
    themes: BTreeMap<String, Theme>,
    #[serde(default)]
    units: Units,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Screen {
    media: String,
    modes: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Theme {
    modes: BTreeMap<String, String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Units {
    #[serde(default)]
    exact: BTreeMap<String, String>,
    #[serde(default)]
    prefix: BTreeMap<String, String>,
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| format!("Missing or invalid {k}"))
}
fn array<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>> {
    v[k].as_array()
        .ok_or_else(|| format!("Missing or invalid {k}"))
}
fn name(s: &str) -> Result<String> {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        Err(format!("Empty CSS name for {s:?}"))
    } else {
        Ok(if out.starts_with(|c: char| c.is_ascii_digit()) {
            format!("n-{out}")
        } else {
            out
        })
    }
}
fn quoted(s: &str) -> String {
    let mut result = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            c if c.is_control() || c == '<' => result.push_str(&format!("\\{:x} ", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
struct Model<'a> {
    config: &'a Config,
    vars: BTreeMap<String, &'a Value>,
    names: BTreeMap<String, String>,
    defaults: BTreeMap<String, String>,
    collections: BTreeMap<String, &'a Value>,
}
impl<'a> Model<'a> {
    fn new(snapshot: &'a Value, config: &'a Config) -> Result<Self> {
        if snapshot["format"] != "crossfigma.snapshot" || snapshot["version"] != 1 {
            return Err("Expected crossfigma.snapshot version 1".into());
        }
        let mut m = Self {
            config,
            vars: BTreeMap::new(),
            names: BTreeMap::new(),
            defaults: BTreeMap::new(),
            collections: BTreeMap::new(),
        };
        for c in array(snapshot, "collections")? {
            let id = field(c, "id")?.to_owned();
            let default = field(c, "defaultModeId")?;
            if !array(c, "modes")?.iter().any(|v| v["modeId"] == default) {
                return Err(format!("Unknown default mode in {id}"));
            }
            m.defaults.insert(id.clone(), default.to_owned());
            if m.collections.insert(id, c).is_some() {
                return Err("Duplicate collection ID".into());
            }
        }
        let mut used = BTreeSet::new();
        for v in array(snapshot, "variables")? {
            let id = field(v, "id")?.to_owned();
            let n = name(field(v, "name")?)?;
            if !used.insert(n.clone()) {
                return Err(format!("Variable name collision: {n}"));
            }
            if !m
                .collections
                .contains_key(field(v, "variableCollectionId")?)
            {
                return Err(format!("Unknown collection for {id}"));
            }
            m.names.insert(id.clone(), n);
            if m.vars.insert(id, v).is_some() {
                return Err("Duplicate variable ID".into());
            }
        }
        Ok(m)
    }
    fn modes(&self, mapping: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        let mut result = self.defaults.clone();
        for (collection, mode) in mapping {
            let matches: Vec<_> = self
                .collections
                .iter()
                .filter(|(_, c)| c["name"] == *collection)
                .collect();
            if matches.len() != 1 {
                return Err(format!("Missing or ambiguous collection {collection}"));
            }
            let (id, c) = matches[0];
            let modes: Vec<_> = array(c, "modes")?
                .iter()
                .filter(|m| m["name"] == *mode)
                .collect();
            if modes.len() != 1 {
                return Err(format!("Missing or ambiguous mode {collection}/{mode}"));
            }
            result.insert(id.clone(), field(modes[0], "modeId")?.to_owned());
        }
        Ok(result)
    }
    fn unit(&self, n: &str) -> &str {
        self.config
            .units
            .exact
            .get(n)
            .or_else(|| {
                self.config
                    .units
                    .prefix
                    .iter()
                    .filter(|(p, _)| n.starts_with(p.as_str()))
                    .max_by_key(|(p, _)| p.len())
                    .map(|(_, v)| v)
            })
            .map(String::as_str)
            .unwrap_or("")
    }
    fn value(
        &self,
        id: &str,
        modes: &BTreeMap<String, String>,
        visiting: &mut BTreeSet<String>,
    ) -> Result<String> {
        if !visiting.insert(id.to_owned()) {
            return Err(format!("Variable alias cycle at {id}"));
        }
        let v = self
            .vars
            .get(id)
            .ok_or_else(|| format!("Unresolved variable {id}"))?;
        let mode = &modes[field(v, "variableCollectionId")?];
        let value = &v["valuesByMode"][mode];
        let out = if value["type"] == "VARIABLE_ALIAS" {
            let target = field(value, "id")?;
            self.value(target, modes, visiting)?;
            format!("var(--{})", self.names[target])
        } else {
            match field(v, "resolvedType")? {
                "FLOAT" => format!(
                    "{}{}",
                    value
                        .as_f64()
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| format!("Invalid numeric value for {id}"))?,
                    self.unit(field(v, "name")?)
                ),
                "STRING" => quoted(
                    value
                        .as_str()
                        .ok_or_else(|| format!("Invalid string for {id}"))?,
                ),
                "BOOLEAN" => value
                    .as_bool()
                    .ok_or_else(|| format!("Invalid boolean for {id}"))?
                    .to_string(),
                "COLOR" => {
                    let channel = |k: &str| -> Result<f64> {
                        value[k]
                            .as_f64()
                            .filter(|x| (0.0..=1.0).contains(x))
                            .ok_or_else(|| format!("Invalid color {id}/{k}"))
                    };
                    format!(
                        "rgb({} {} {} / {})",
                        (channel("r")? * 255.0).round(),
                        (channel("g")? * 255.0).round(),
                        (channel("b")? * 255.0).round(),
                        channel("a")?
                    )
                }
                t => return Err(format!("Unsupported variable type {t}")),
            }
        };
        visiting.remove(id);
        Ok(out)
    }
    fn values(&self, modes: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        self.vars
            .keys()
            .map(|id| {
                Ok((
                    self.names[id].clone(),
                    self.value(id, modes, &mut BTreeSet::new())?,
                ))
            })
            .collect()
    }
}
fn block(selector: &str, values: &BTreeMap<String, String>) -> String {
    format!(
        "{selector} {{\n{}}}\n",
        values
            .iter()
            .map(|(k, v)| format!("  --{k}: {v};\n"))
            .collect::<String>()
    )
}
fn generate(snapshot: &Value, config: &Config) -> Result<BTreeMap<String, String>> {
    for unit in config
        .units
        .exact
        .values()
        .chain(config.units.prefix.values())
    {
        if !unit.chars().all(|c| c.is_ascii_alphabetic() || c == '%') {
            return Err(format!("Invalid unit {unit}"));
        }
    }
    let m = Model::new(snapshot, config)?;
    let base = m.values(&m.defaults)?;
    let mut files = BTreeMap::new();
    files.insert("globals.gen.xcss".into(), block(":root", &base));
    let overrides = |mapping: &BTreeMap<String, String>| -> Result<BTreeMap<String, String>> {
        Ok(m.values(&m.modes(mapping)?)?
            .into_iter()
            .filter(|(k, v)| base.get(k) != Some(v))
            .collect())
    };
    for (screen, s) in &config.screens {
        if name(screen)? != *screen
            || screen == "."
            || s.media.trim().is_empty()
            || s.media.contains(['{', '}', ';'])
        {
            return Err(format!("Invalid screen name or media query: {screen}"));
        }
        let inner = block(":root", &overrides(&s.modes)?)
            .lines()
            .map(|l| format!("  {l}\n"))
            .collect::<String>();
        files.insert(
            format!("{screen}-media.gen.xcss"),
            format!("@media {} {{\n{inner}}}\n", s.media),
        );
    }
    let mut themes = String::new();
    for (n, t) in &config.themes {
        themes.push_str(&block(
            &format!("[data-theme={}]", quoted(n)),
            &overrides(&t.modes)?,
        ));
    }
    files.insert("themes.xcss".into(), themes);
    let mut typography = String::new();
    let mut names = BTreeSet::new();
    for style in array(&snapshot["styles"], "text")? {
        let n = name(field(style, "name")?)?;
        if !names.insert(n.clone()) {
            return Err(format!("Typography name collision: {n}"));
        }
        let mut props = BTreeMap::<String, String>::new();
        if let Some(family) = style["fontName"]["family"].as_str() {
            props.insert("font-family".into(), quoted(family));
        }
        if let Some(font) = style["fontName"]["style"].as_str() {
            let font = font.to_ascii_lowercase();
            let weight = if font.contains("thin") {
                100
            } else if font.contains("extra light") || font.contains("extralight") {
                200
            } else if font.contains("light") {
                300
            } else if font.contains("semi") || font.contains("demi") {
                600
            } else if font.contains("extra bold") || font.contains("extrabold") {
                800
            } else if font.contains("black") || font.contains("heavy") {
                900
            } else if font.contains("bold") {
                700
            } else if font.contains("medium") {
                500
            } else {
                400
            };
            props.insert("font-weight".into(), weight.to_string());
            props.insert(
                "font-style".into(),
                if font.contains("italic") {
                    "italic"
                } else if font.contains("oblique") {
                    "oblique"
                } else {
                    "normal"
                }
                .into(),
            );
        }
        for (source, target) in [
            ("fontSize", "font-size"),
            ("paragraphIndent", "text-indent"),
        ] {
            if let Some(v) = style[source].as_f64() {
                props.insert(target.into(), format!("{v}px"));
            }
        }
        for (source, target) in [
            ("lineHeight", "line-height"),
            ("letterSpacing", "letter-spacing"),
        ] {
            let v = &style[source];
            if v.is_null() {
                continue;
            }
            let out = match v["unit"].as_str() {
                Some("AUTO") => "normal".into(),
                Some("PIXELS") => format!(
                    "{}px",
                    v["value"].as_f64().ok_or("Invalid typography length")?
                ),
                Some("PERCENT") => {
                    let x = v["value"].as_f64().ok_or("Invalid typography percentage")? / 100.0;
                    if source == "lineHeight" {
                        x.to_string()
                    } else {
                        format!("{x}em")
                    }
                }
                _ => return Err(format!("Unsupported {source} in {n}")),
            };
            props.insert(target.into(), out);
        }
        for (source, target, choices) in [
            (
                "textCase",
                "text-transform",
                vec![
                    ("ORIGINAL", "none"),
                    ("UPPER", "uppercase"),
                    ("LOWER", "lowercase"),
                    ("TITLE", "capitalize"),
                ],
            ),
            (
                "textDecoration",
                "text-decoration",
                vec![
                    ("NONE", "none"),
                    ("UNDERLINE", "underline"),
                    ("STRIKETHROUGH", "line-through"),
                ],
            ),
        ] {
            if let Some(v) = style[source].as_str() {
                if let Some((_, css)) = choices.iter().find(|(k, _)| *k == v) {
                    props.insert(target.into(), css.to_string());
                } else {
                    eprintln!("warning: unsupported {source} {v} in {n}");
                }
            }
        }
        if let Some(bindings) = style["boundVariables"].as_object() {
            for (source, binding) in bindings {
                let target = match source.as_str() {
                    "fontSize" => "font-size",
                    "paragraphIndent" => "text-indent",
                    _ => {
                        eprintln!("warning: unsupported typography binding {source} in {n}");
                        continue;
                    }
                };
                let id = field(binding, "id")?;
                m.value(id, &m.defaults, &mut BTreeSet::new())?;
                props.insert(target.into(), format!("var(--{})", m.names[id]));
            }
        }
        for key in [
            "paragraphSpacing",
            "listSpacing",
            "hangingPunctuation",
            "hangingList",
            "leadingTrim",
            "textWrapStyle",
        ] {
            if !style[key].is_null() {
                eprintln!("warning: unsupported typography property {key} in {n}");
            }
        }
        typography.push_str(&format!(
            "@util {n} {{\n{}}}\n",
            props
                .iter()
                .map(|(k, v)| format!("  {k}: {v};\n"))
                .collect::<String>()
        ));
    }
    files.insert("typography.gen.xcss".into(), typography);
    for key in ["paint", "effect", "grid"] {
        if snapshot["styles"][key]
            .as_array()
            .is_some_and(|v| !v.is_empty())
        {
            eprintln!("warning: {key} styles are not imported");
        }
    }
    for (id, c) in &m.collections {
        for mode in array(c, "modes")? {
            let mid = field(mode, "modeId")?;
            if mid == m.defaults[id] {
                continue;
            }
            let mapped = config.screens.values().any(|s| {
                s.modes.get(c["name"].as_str().unwrap_or(""))
                    == mode["name"].as_str().map(|s| s.to_string()).as_ref()
            }) || config.themes.values().any(|t| {
                t.modes.get(c["name"].as_str().unwrap_or(""))
                    == mode["name"].as_str().map(|s| s.to_string()).as_ref()
            });
            if !mapped {
                eprintln!("warning: unused mode {}/{}", c["name"], mode["name"]);
            }
        }
    }
    Ok(files)
}
const MANIFEST: &str = ".cross-figma.json";
#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    names: BTreeMap<String, String>,
}
fn rewrite(text: &str, names: &BTreeMap<String, String>) -> String {
    let parsed = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
        xcss_source::FileId("update.xcss".into()),
        0,
        text.to_owned(),
    )));
    let mut result = String::new();
    let mut cursor = 0;
    for token in parsed.tokens.iter() {
        if token.kind != xcss_syntax::TokenKind::Word {
            continue;
        }
        let word = &text[token.range.start..token.range.end];
        if let Some(replacement) = names.get(word) {
            result.push_str(&text[cursor..token.range.start]);
            result.push_str(replacement);
            cursor = token.range.end;
        }
    }
    result.push_str(&text[cursor..]);
    result
}
fn scan_sources(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if [
                "node_modules",
                ".git",
                ".cache",
                "dist",
                "target",
                "coverage",
                ".xcss",
            ]
            .iter()
            .any(|n| entry.file_name() == *n)
            {
                continue;
            }
            scan_sources(&entry.path(), files)?;
        } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "xcss") {
            files.push(entry.path());
        }
    }
    Ok(())
}
fn import(bytes: &[u8], config: &Config, root: &Path, update: bool) -> Result<()> {
    let snapshot: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let files = generate(&snapshot, config)?;
    let names = Model::new(&snapshot, config)?.names;
    let output = root.join(&config.output);
    let manifest_path = output.join(MANIFEST);
    let mut writes = BTreeMap::new();
    if update {
        let previous: Manifest =
            serde_json::from_slice(&fs::read(&manifest_path).map_err(|e| {
                format!(
                    "{}: {e}; import the previous snapshot first to establish variable IDs",
                    manifest_path.display()
                )
            })?)
            .map_err(|e| e.to_string())?;
        if previous.version != 1 {
            return Err("Unsupported CrossFigma manifest version".into());
        }
        let mut sources = Vec::new();
        scan_sources(root, &mut sources)?;
        let mut texts = Vec::new();
        for path in sources {
            if files.keys().any(|n| output.join(n) == path) {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            texts.push((path, text));
        }
        let mut first = BTreeMap::new();
        let mut second = BTreeMap::new();
        for (id, old) in &previous.names {
            let Some(new) = names.get(id) else {
                eprintln!("warning: deleted variable --{old} ({id}); references preserved");
                continue;
            };
            if old == new {
                continue;
            }
            let encoded = id
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let mut temporary = format!("--xcss-update-id-{encoded}");
            while texts.iter().any(|(_, t)| t.contains(&temporary))
                || names.values().any(|n| format!("--{n}") == temporary)
                || second.contains_key(&temporary)
            {
                temporary.push('x');
            }
            first.insert(format!("--{old}"), temporary.clone());
            second.insert(temporary, format!("--{new}"));
        }
        // A destination already declared outside the set being renamed is ambiguous.
        for (_, text) in &texts {
            let parsed = xcss_syntax::parse(std::sync::Arc::new(xcss_source::SourceFile::new(
                xcss_source::FileId("update.xcss".into()),
                0,
                text.clone(),
            )));
            let tokens: Vec<_> = parsed
                .tokens
                .iter()
                .filter(|t| {
                    !matches!(
                        t.kind,
                        xcss_syntax::TokenKind::Whitespace | xcss_syntax::TokenKind::Comment
                    )
                })
                .collect();
            for pair in tokens.windows(2) {
                let word = &text[pair[0].range.start..pair[0].range.end];
                if &text[pair[1].range.start..pair[1].range.end] == ":"
                    && second.values().any(|n| n == word)
                    && !first.contains_key(word)
                {
                    return Err(format!("Variable destination already declared: {word}"));
                }
            }
        }
        for (path, text) in texts {
            let next = rewrite(&rewrite(&text, &first), &second);
            if next != text {
                writes.insert(path, next);
            }
        }
    }
    for (name, body) in files {
        writes.insert(output.join(name), body);
    }
    writes.insert(
        manifest_path,
        serde_json::to_string_pretty(&Manifest { version: 1, names }).map_err(|e| e.to_string())?,
    );
    super::write_transaction(writes.into_iter().collect())
}
pub fn run(args: &[String]) -> Result<()> {
    let usage = "usage: xcss cross-figma import <snapshot.json> [--root <directory>] | xcss cross-figma receive [--root <directory>] | xcss cross-figma update [snapshot.json] [--root <directory>]";
    let command = args.first().ok_or(usage)?;
    let mut index = 1;
    let input = if command == "import"
        || (command == "update" && args.get(1).is_some_and(|a| a != "--root"))
    {
        index = 2;
        Some(args.get(1).ok_or(usage)?)
    } else if command == "receive" || command == "update" {
        None
    } else {
        return Err(usage.into());
    };
    let mut root = env::current_dir().map_err(|e| e.to_string())?;
    if args.len() > index {
        if args.len() != index + 2 || args[index] != "--root" {
            return Err(usage.into());
        }
        root = PathBuf::from(&args[index + 1]);
    }
    let doc = xcss_resolver::ProjectConfig::load(&root)?.document;
    let config: Config =
        serde_json::from_value(doc["cross-figma"].clone()).map_err(|e| e.to_string())?;
    if config.output.trim().is_empty() {
        return Err("cross-figma.output must not be empty".into());
    }
    if let Some(input) = input {
        return import(
            &fs::read(input).map_err(|e| e.to_string())?,
            &config,
            &root,
            command == "update",
        );
    }
    receive(&config, &root, command == "update")
}
fn receive(config: &Config, root: &Path, update: bool) -> Result<()> {
    use tiny_http::{Header, Response, Server, StatusCode};
    let servers = vec![
        Server::http("127.0.0.1:38473").map_err(|e| e.to_string())?,
        Server::http("[::1]:38473").map_err(|e| e.to_string())?,
    ];
    println!("Waiting for CrossFigma at http://localhost:38473 (Ctrl+C to stop)");
    loop {
        for server in &servers {
            if let Some(mut request) = server
                .recv_timeout(std::time::Duration::from_millis(100))
                .map_err(|e| e.to_string())?
            {
                let mut success = false;
                let (status, body) = match (request.method().as_str(), request.url()) {
                    ("OPTIONS", _) => (204, String::new()),
                    ("GET", "/health") => (
                        200,
                        r#"{"service":"CrossBreeze","protocolVersion":1}"#.into(),
                    ),
                    ("POST", "/design-tokens") => {
                        let json = request.headers().iter().any(|h| {
                            h.field.equiv("Content-Type")
                                && h.value.as_str().split(';').next().is_some_and(|v| {
                                    v.trim().eq_ignore_ascii_case("application/json")
                                })
                        });
                        if !json {
                            (415, "Expected application/json".into())
                        } else {
                            let mut bytes = Vec::new();
                            let read = request
                                .as_reader()
                                .take(32 * 1024 * 1024 + 1)
                                .read_to_end(&mut bytes);
                            if bytes.len() > 32 * 1024 * 1024 {
                                (413, "Snapshot exceeds 32 MiB".into())
                            } else {
                                match read
                                    .map_err(|e| e.to_string())
                                    .and_then(|_| import(&bytes, config, root, update))
                                {
                                    Ok(()) => {
                                        success = true;
                                        (200, "Imported".into())
                                    }
                                    Err(e) => (400, e),
                                }
                            }
                        }
                    }
                    _ => (404, "Not found".into()),
                };
                let mut response = Response::from_string(body).with_status_code(StatusCode(status));
                for (k, v) in [
                    ("Access-Control-Allow-Origin", "*"),
                    ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
                    ("Access-Control-Allow-Headers", "Content-Type"),
                    ("Access-Control-Allow-Private-Network", "true"),
                    (
                        "Content-Type",
                        if request.url() == "/health" {
                            "application/json"
                        } else {
                            "text/plain; charset=utf-8"
                        },
                    ),
                ] {
                    response.add_header(Header::from_bytes(k, v).unwrap());
                }
                request.respond(response).map_err(|e| e.to_string())?;
                if success {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::json!({"format":"crossfigma.snapshot","version":1,"collections":[{"id":"c","name":"Tokens","defaultModeId":"base","modes":[{"modeId":"base","name":"Base"},{"modeId":"small","name":"Small"}]}],"variables":[{"id":"v","name":"Spacing/Base","variableCollectionId":"c","resolvedType":"FLOAT","valuesByMode":{"base":16,"small":8}},{"id":"alias","name":"Spacing/Alias","variableCollectionId":"c","resolvedType":"FLOAT","valuesByMode":{"base":{"type":"VARIABLE_ALIAS","id":"v"},"small":{"type":"VARIABLE_ALIAS","id":"v"}}}],"styles":{"text":[{"name":"Heading/Large","fontName":{"family":"Inter","style":"Bold"},"fontSize":24,"lineHeight":{"unit":"PERCENT","value":120}}],"paint":[],"effect":[],"grid":[]}})
    }
    fn config() -> Config {
        serde_json::from_value(serde_json::json!({"output":"generated","screens":{"tablet":{"media":"(width < 1000px)","modes":{"Tokens":"Small"}}},"themes":{"compact":{"modes":{"Tokens":"Small"}}},"units":{"prefix":{"Spacing/":"px"}}})).unwrap()
    }
    #[test]
    fn update_swaps_names_and_preserves_deleted_references() {
        let root = std::env::temp_dir().join(format!("xcss-update-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        let cfg = config();
        let old = fixture();
        import(&serde_json::to_vec(&old).unwrap(), &cfg, &root, false).unwrap();
        let source = ".a { width: --spacing-base; height: var(--spacing-alias); content: \"--spacing-base\"; /* --spacing-base */ padding: --spacing-base-extra; }";
        fs::write(root.join("src/a.xcss"), source).unwrap();
        let mut next = old.clone();
        next["variables"][0]["name"] = "Spacing/Alias".into();
        next["variables"][1]["name"] = "Spacing/Base".into();
        let bytes = serde_json::to_vec(&next).unwrap();
        import(&bytes, &cfg, &root, true).unwrap();
        let changed = fs::read_to_string(root.join("src/a.xcss")).unwrap();
        assert!(changed.contains("width: --spacing-alias; height: var(--spacing-base)"));
        assert!(changed.contains("\"--spacing-base\"; /* --spacing-base */"));
        assert!(changed.contains("--spacing-base-extra"));
        import(&bytes, &cfg, &root, true).unwrap();
        assert_eq!(
            changed,
            fs::read_to_string(root.join("src/a.xcss")).unwrap()
        );
        next["variables"].as_array_mut().unwrap().pop();
        import(&serde_json::to_vec(&next).unwrap(), &cfg, &root, true).unwrap();
        assert_eq!(
            changed,
            fs::read_to_string(root.join("src/a.xcss")).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn update_requires_baseline_and_rejects_destination_conflicts() {
        let root = std::env::temp_dir().join(format!("xcss-update-errors-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let bytes = serde_json::to_vec(&fixture()).unwrap();
        assert!(
            import(&bytes, &config(), &root, true)
                .unwrap_err()
                .contains("previous snapshot")
        );
        import(&bytes, &config(), &root, false).unwrap();
        let text = ":root { --new-name: 1; } .a { width: --spacing-base; }";
        fs::write(root.join("a.xcss"), text).unwrap();
        let mut next = fixture();
        next["variables"][0]["name"] = "new-name".into();
        assert!(
            import(&serde_json::to_vec(&next).unwrap(), &config(), &root, true)
                .unwrap_err()
                .contains("already declared")
        );
        assert_eq!(fs::read_to_string(root.join("a.xcss")).unwrap(), text);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn generates_modes_aliases_and_typography() {
        let output = generate(&fixture(), &config()).unwrap();
        assert!(output["globals.gen.xcss"].contains("--spacing-base: 16px;"));
        assert!(output["globals.gen.xcss"].contains("--spacing-alias: var(--spacing-base);"));
        assert!(output["tablet-media.gen.xcss"].contains("--spacing-base: 8px;"));
        assert!(!output["tablet-media.gen.xcss"].contains("--spacing-alias:"));
        assert!(output["themes.xcss"].contains("[data-theme=\"compact\"]"));
        assert!(output["typography.gen.xcss"].contains("@util heading-large"));
        assert!(output["typography.gen.xcss"].contains("line-height: 1.2;"));
        assert_eq!(output, generate(&fixture(), &config()).unwrap());
    }
    #[test]
    fn rejects_cycles_collisions_and_missing_modes() {
        let mut snapshot = fixture();
        snapshot["variables"][0]["valuesByMode"]["base"] =
            serde_json::json!({"type":"VARIABLE_ALIAS","id":"alias"});
        assert!(
            generate(&snapshot, &config())
                .unwrap_err()
                .contains("cycle")
        );
        let mut snapshot = fixture();
        snapshot["variables"][1]["name"] = "spacing base".into();
        assert!(
            generate(&snapshot, &config())
                .unwrap_err()
                .contains("collision")
        );
        let mut cfg = config();
        cfg.screens
            .get_mut("tablet")
            .unwrap()
            .modes
            .insert("Tokens".into(), "Missing".into());
        assert!(generate(&fixture(), &cfg).unwrap_err().contains("mode"));
    }
    #[test]
    fn exact_units_override_prefixes() {
        let mut cfg = config();
        cfg.units.exact.insert("Spacing/Base".into(), "".into());
        assert!(
            generate(&fixture(), &cfg).unwrap()["globals.gen.xcss"].contains("--spacing-base: 16;")
        );
    }
}
