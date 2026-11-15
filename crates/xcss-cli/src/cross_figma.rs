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
fn import(bytes: &[u8], config: &Config, root: &Path) -> Result<()> {
    let snapshot: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let files = generate(&snapshot, config)?;
    let output = root.join(&config.output);
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    for (name, body) in files {
        let path = output.join(name);
        fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("Generated {}", path.display());
    }
    Ok(())
}
pub fn run(args: &[String]) -> Result<()> {
    let usage = "usage: xcss cross-figma import <snapshot.json> [--root <directory>] | xcss cross-figma receive [--root <directory>]";
    let command = args.first().ok_or(usage)?;
    let mut index = 1;
    let input = if command == "import" {
        index = 2;
        Some(args.get(1).ok_or(usage)?)
    } else if command == "receive" {
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
    let config_path = root.join("xcss.config.json");
    let doc: Value = serde_json::from_slice(
        &fs::read(&config_path).map_err(|e| format!("{}: {e}", config_path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let config: Config =
        serde_json::from_value(doc["cross-figma"].clone()).map_err(|e| e.to_string())?;
    if config.output.trim().is_empty() {
        return Err("cross-figma.output must not be empty".into());
    }
    if let Some(input) = input {
        return import(&fs::read(input).map_err(|e| e.to_string())?, &config, &root);
    }
    receive(&config, &root)
}
fn receive(config: &Config, root: &Path) -> Result<()> {
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
                                    .and_then(|_| import(&bytes, config, root))
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
