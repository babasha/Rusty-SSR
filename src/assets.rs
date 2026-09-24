//! Preload tags for the code-split chunks a render used.
//!
//! A server render knows which lazy screens it drew; the browser does not,
//! until the entry bundle has downloaded and run and asked for them. For a
//! page whose main content is a lazy screen that is one full round trip of
//! waiting, after the entry, before the content can hydrate — measured on a
//! production listing page at ~0.5 s on a throttled phone, then a second step
//! for the chunk that one imports.
//!
//! [`ViteManifest`] closes the gap from the client build's
//! `.vite/manifest.json`: given the modules a render reported (see
//! [`Rendered`](crate::v8_pool::Rendered)), it writes a
//! `<link rel="modulepreload">` for each chunk the browser is going to need,
//! the chunks those import, and a stylesheet link for any CSS they carry, so
//! all of it downloads alongside the entry instead of after it.
//!
//! A module is found by its manifest key, and — when the build also wrote
//! `ssr-manifest.json` (`build.ssrManifest`) — by the chunk that file says it
//! landed in. The second route is not optional in practice: when two chunks
//! share a lazily imported module, Rollup puts it in a chunk that is nobody's
//! facade, and the manifest keys that chunk `_Name-hash.js` instead of by the
//! module's path. Measured on a real app: its most important lazy screen was
//! exactly that case. [`from_dir`](ViteManifest::from_dir) reads both.
//!
//! What it leaves out is what the document already has: every chunk reachable
//! from an entry by static imports. Vite writes those tags into the HTML
//! itself, and repeating them would be bytes on every page for nothing.
//!
//! ```rust,no_run
//! use rusty_ssr::assets::ViteManifest;
//!
//! let manifest = ViteManifest::from_path("dist/.vite/manifest.json").unwrap();
//! let tags = manifest.preload_links(&["src/screens/Detail.tsx"]);
//! // → <link rel="modulepreload" crossorigin href="/assets/Detail-3f2a.js">…
//! ```

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

use crate::error::{SsrError, SsrResult};

/// One entry of a Vite manifest — only the fields preloading needs.
#[derive(Debug, Clone, Deserialize)]
struct Chunk {
    file: String,
    #[serde(default)]
    imports: Vec<String>,
    #[serde(default)]
    css: Vec<String>,
    #[serde(default, rename = "isEntry")]
    is_entry: bool,
}

/// A client build's manifest, read once and asked per page.
#[derive(Debug, Clone)]
pub struct ViteManifest {
    chunks: HashMap<String, Chunk>,
    /// Files the document already references: every entry and everything it
    /// imports statically, with their CSS.
    in_document: HashSet<String>,
    /// Module id → the keys of the chunks it landed in, from ssr-manifest.json.
    by_module: HashMap<String, Vec<String>>,
    base: String,
}

/// The files to preload for a page, in document order, with the base applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preloads {
    /// JavaScript chunks, for `<link rel="modulepreload">`.
    pub scripts: Vec<String>,
    /// Stylesheets the chunks carry, for `<link rel="stylesheet">`.
    pub styles: Vec<String>,
}

impl ViteManifest {
    /// Read `manifest.json` from a build's `.vite` directory, and
    /// `ssr-manifest.json` beside it when the build wrote one (see the module
    /// note for why it matters). A missing SSR manifest is not an error; an
    /// unreadable one is.
    pub fn from_dir<P: AsRef<Path>>(dir: P) -> SsrResult<Self> {
        let dir = dir.as_ref();
        let manifest = Self::from_path(dir.join("manifest.json"))?;
        let ssr = dir.join("ssr-manifest.json");
        if !ssr.exists() {
            return Ok(manifest);
        }
        let raw = std::fs::read_to_string(&ssr).map_err(|e| {
            SsrError::Template(format!("Failed to read SSR manifest {:?}: {}", ssr, e))
        })?;
        manifest.with_ssr_manifest(&raw)
    }

    /// Read a manifest from disk (`<outDir>/.vite/manifest.json`).
    pub fn from_path<P: AsRef<Path>>(path: P) -> SsrResult<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|e| {
            SsrError::Template(format!("Failed to read manifest {:?}: {}", path, e))
        })?;
        Self::parse(&raw)
    }

    /// Parse a manifest from its JSON text.
    pub fn parse(json: &str) -> SsrResult<Self> {
        let chunks: HashMap<String, Chunk> = serde_json::from_str(json)
            .map_err(|e| SsrError::Template(format!("Failed to parse manifest JSON: {}", e)))?;

        let mut in_document = HashSet::new();
        let mut seen = HashSet::new();
        let mut stack: Vec<&str> = chunks
            .iter()
            .filter(|(_, c)| c.is_entry)
            .map(|(k, _)| k.as_str())
            .collect();
        while let Some(key) = stack.pop() {
            if !seen.insert(key) {
                continue;
            }
            let Some(chunk) = chunks.get(key) else { continue };
            in_document.insert(chunk.file.clone());
            in_document.extend(chunk.css.iter().cloned());
            stack.extend(chunk.imports.iter().map(String::as_str));
        }

        Ok(Self { chunks, in_document, by_module: HashMap::new(), base: "/".to_string() })
    }

    /// Add the module → chunk index from Vite's `ssr-manifest.json` (module id
    /// → the public paths of the files it landed in). Only JavaScript files
    /// that are chunks of this manifest are kept; CSS comes with the chunk.
    pub fn with_ssr_manifest(mut self, json: &str) -> SsrResult<Self> {
        let raw: HashMap<String, Vec<String>> = serde_json::from_str(json).map_err(|e| {
            SsrError::Template(format!("Failed to parse SSR manifest JSON: {}", e))
        })?;
        let by_file: HashMap<&str, &str> =
            self.chunks.iter().map(|(k, c)| (c.file.as_str(), k.as_str())).collect();
        let mut by_module = HashMap::new();
        for (module, files) in raw {
            let keys: Vec<String> = files
                .iter()
                .filter(|f| f.ends_with(".js") || f.ends_with(".mjs"))
                .filter_map(|f| {
                    // The paths carry the build's base ("/assets/x.js"); the
                    // manifest's files do not ("assets/x.js").
                    let bare = f.trim_start_matches('/');
                    by_file.get(bare).copied().or_else(|| {
                        by_file
                            .iter()
                            .find(|(file, _)| f.ends_with(&format!("/{file}")))
                            .map(|(_, k)| *k)
                    })
                })
                .map(str::to_string)
                .collect();
            if !keys.is_empty() {
                by_module.insert(module, keys);
            }
        }
        self.by_module = by_module;
        Ok(self)
    }

    /// The public path the files are served under — Vite's `base`. Defaults
    /// to `/`; a trailing slash is added if missing.
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        let mut base = base.into();
        if !base.ends_with('/') {
            base.push('/');
        }
        self.base = base;
        self
    }

    /// Whether this module can be found — as a manifest key, or through the
    /// SSR manifest.
    pub fn contains(&self, module: &str) -> bool {
        self.chunks.contains_key(module) || self.by_module.contains_key(module)
    }

    /// What to preload for a page that used `modules` (manifest keys, as
    /// reported by the render). Unknown keys are skipped: a module the client
    /// build has no chunk for — server-only code, a module merged into the
    /// entry — has nothing to preload.
    pub fn preloads<S: AsRef<str>>(&self, modules: &[S]) -> Preloads {
        let mut out = Preloads::default();
        let mut seen_chunks = HashSet::new();
        let mut seen_files = HashSet::new();
        for module in modules {
            let module = module.as_ref();
            if let Some((key, _)) = self.chunks.get_key_value(module) {
                self.walk(key, &mut seen_chunks, &mut seen_files, &mut out);
            } else if let Some(keys) = self.by_module.get(module) {
                for key in keys {
                    self.walk(key, &mut seen_chunks, &mut seen_files, &mut out);
                }
            }
        }
        out
    }

    fn walk<'a>(
        &'a self,
        key: &'a str,
        seen_chunks: &mut HashSet<&'a str>,
        seen_files: &mut HashSet<&'a str>,
        out: &mut Preloads,
    ) {
        if !seen_chunks.insert(key) {
            return;
        }
        let Some(chunk) = self.chunks.get(key) else { return };
        if !self.in_document.contains(&chunk.file) && seen_files.insert(&chunk.file) {
            out.scripts.push(format!("{}{}", self.base, chunk.file));
        }
        for css in &chunk.css {
            if !self.in_document.contains(css) && seen_files.insert(css) {
                out.styles.push(format!("{}{}", self.base, css));
            }
        }
        for import in &chunk.imports {
            self.walk(import, seen_chunks, seen_files, out);
        }
    }

    /// [`preloads`](Self::preloads) as HTML, ready for `<head>`: stylesheets
    /// first (they block rendering, so the sooner they are asked for the
    /// better), then one `modulepreload` per chunk, in the form Vite writes
    /// its own. Empty when there is nothing to add.
    pub fn preload_links<S: AsRef<str>>(&self, modules: &[S]) -> String {
        let p = self.preloads(modules);
        let mut html = String::new();
        for href in &p.styles {
            html.push_str("<link rel=\"stylesheet\" crossorigin href=\"");
            push_attr(&mut html, href);
            html.push_str("\">");
        }
        for href in &p.scripts {
            html.push_str("<link rel=\"modulepreload\" crossorigin href=\"");
            push_attr(&mut html, href);
            html.push_str("\">");
        }
        html
    }
}

/// Escape for a double-quoted attribute. Manifest paths are build output, not
/// user input, but a stray `"` would still end the attribute.
fn push_attr(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real Vite manifest: an entry with a static vendor
    /// import, two lazy screens sharing a helper chunk, one with CSS.
    const MANIFEST: &str = r#"{
        "index.html": {
            "file": "assets/index-a1.js", "isEntry": true,
            "imports": ["_vendor-b2.js"], "css": ["assets/index-c3.css"],
            "dynamicImports": ["src/Detail.tsx", "src/Map.tsx"]
        },
        "_vendor-b2.js": { "file": "assets/vendor-b2.js" },
        "_shared-d4.js": { "file": "assets/shared-d4.js", "imports": ["_vendor-b2.js"] },
        "src/Detail.tsx": {
            "file": "assets/Detail-e5.js", "isDynamicEntry": true,
            "imports": ["_shared-d4.js", "index.html"]
        },
        "src/Map.tsx": {
            "file": "assets/Map-f6.js", "isDynamicEntry": true,
            "imports": ["_shared-d4.js"], "css": ["assets/Map-f6.css"]
        }
    }"#;

    fn manifest() -> ViteManifest {
        ViteManifest::parse(MANIFEST).unwrap()
    }

    #[test]
    fn a_lazy_screen_brings_its_chunk_and_what_it_imports() {
        let p = manifest().preloads(&["src/Detail.tsx"]);
        assert_eq!(p.scripts, vec!["/assets/Detail-e5.js", "/assets/shared-d4.js"]);
        assert!(p.styles.is_empty());
    }

    #[test]
    fn nothing_the_document_already_loads_is_repeated() {
        // Detail imports the entry and the vendor chunk, both already in the
        // HTML Vite wrote.
        let p = manifest().preloads(&["src/Detail.tsx", "src/Map.tsx"]);
        for f in &p.scripts {
            assert!(!f.contains("index-a1") && !f.contains("vendor-b2"), "{f}");
        }
        assert!(!p.styles.iter().any(|s| s.contains("index-c3")));
    }

    #[test]
    fn a_chunk_shared_by_two_screens_is_listed_once() {
        let p = manifest().preloads(&["src/Detail.tsx", "src/Map.tsx"]);
        assert_eq!(
            p.scripts,
            vec!["/assets/Detail-e5.js", "/assets/shared-d4.js", "/assets/Map-f6.js"]
        );
        assert_eq!(p.styles, vec!["/assets/Map-f6.css"]);
    }

    #[test]
    fn an_unknown_module_is_skipped_not_an_error() {
        let p = manifest().preloads(&["src/serverOnly.ts", "src/Map.tsx"]);
        assert_eq!(p.scripts, vec!["/assets/Map-f6.js", "/assets/shared-d4.js"]);
    }

    #[test]
    fn no_modules_means_no_tags() {
        assert_eq!(manifest().preload_links::<&str>(&[]), "");
    }

    #[test]
    fn tags_are_written_the_way_vite_writes_its_own() {
        let html = manifest().preload_links(&["src/Map.tsx"]);
        assert_eq!(
            html,
            "<link rel=\"stylesheet\" crossorigin href=\"/assets/Map-f6.css\">\
             <link rel=\"modulepreload\" crossorigin href=\"/assets/Map-f6.js\">\
             <link rel=\"modulepreload\" crossorigin href=\"/assets/shared-d4.js\">"
        );
    }

    #[test]
    fn the_base_prefixes_every_file() {
        let p = manifest().with_base("/static").preloads(&["src/Map.tsx"]);
        assert_eq!(p.scripts[0], "/static/assets/Map-f6.js");
        assert_eq!(p.styles[0], "/static/assets/Map-f6.css");
    }

    #[test]
    fn a_cycle_in_the_imports_terminates() {
        let m = ViteManifest::parse(
            r#"{ "a": { "file": "a.js", "imports": ["b"] }, "b": { "file": "b.js", "imports": ["a"] } }"#,
        )
        .unwrap();
        assert_eq!(m.preloads(&["a"]).scripts, vec!["/a.js", "/b.js"]);
    }

    /// Rollup put a lazily imported module in a shared chunk: the manifest keys
    /// the chunk by its file name, and only the SSR manifest says which module
    /// is in it.
    #[test]
    fn a_module_in_a_shared_chunk_is_found_through_the_ssr_manifest() {
        let m = ViteManifest::parse(
            r#"{
                "index.html": { "file": "assets/index-a1.js", "isEntry": true,
                                "dynamicImports": ["_Detail-e5.js"] },
                "_Detail-e5.js": { "file": "assets/Detail-e5.js", "isDynamicEntry": true,
                                   "imports": ["index.html", "_Ficha-g7.js"] },
                "_Ficha-g7.js": { "file": "assets/Ficha-g7.js", "css": ["assets/Ficha-g7.css"] }
            }"#,
        )
        .unwrap();
        assert!(m.preloads(&["src/Detail.tsx"]).scripts.is_empty(), "not a manifest key");
        let m = m
            .with_ssr_manifest(
                r#"{
                    "src/Detail.tsx": ["/assets/Detail-e5.js"],
                    "src/Ficha.tsx": ["/assets/Ficha-g7.js", "/assets/Ficha-g7.css"],
                    "src/Main.tsx": ["/assets/index-a1.js"]
                }"#,
            )
            .unwrap();
        assert!(m.contains("src/Detail.tsx"));
        let p = m.preloads(&["src/Detail.tsx"]);
        assert_eq!(p.scripts, vec!["/assets/Detail-e5.js", "/assets/Ficha-g7.js"]);
        assert_eq!(p.styles, vec!["/assets/Ficha-g7.css"]);
        // A module that lives in the entry has nothing to preload.
        assert_eq!(m.preloads(&["src/Main.tsx"]), Preloads::default());
    }

    #[test]
    fn a_manifest_key_wins_over_the_ssr_manifest() {
        let m = manifest()
            .with_ssr_manifest(r#"{ "src/Map.tsx": ["/assets/Detail-e5.js"] }"#)
            .unwrap();
        assert_eq!(m.preloads(&["src/Map.tsx"]).scripts[0], "/assets/Map-f6.js");
    }

    #[test]
    fn a_malformed_manifest_is_an_error() {
        assert!(ViteManifest::parse("[]").is_err());
    }
}
