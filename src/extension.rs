//! `.aseprite-extension` packages: a zip with `package.json` plus scripts and
//! themes. Installs are extracted under the config directory with traversal and
//! size limits; scripts run explicitly through the sandboxed Lua engine.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::editor::Editor;

const MAX_FILES: usize = 500;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub author: Option<serde_json::Value>,
    pub description: String,
    pub contributes: Contributes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Contributes {
    pub scripts: Vec<String>,
    pub themes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Extension {
    pub manifest: Manifest,
    pub scripts: Vec<PathBuf>,
    pub themes: Vec<PathBuf>,
}

impl Extension {
    pub fn id(&self) -> &str {
        &self.manifest.name
    }

    pub fn label(&self) -> String {
        if self.manifest.display_name.is_empty() {
            self.id().to_string()
        } else {
            self.manifest.display_name.clone()
        }
    }

    pub fn author(&self) -> String {
        match &self.manifest.author {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Object(m)) => m
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        }
    }

    /// Run every contributed script in manifest order. Returns combined output.
    pub fn run_scripts(&self, editor: &mut Editor) -> Result<String> {
        let mut out = Vec::new();
        for path in &self.scripts {
            let text = crate::script::run_file(editor, path)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("script {}", path.display()))?;
            if !text.is_empty() {
                out.push(text);
            }
        }
        Ok(out.join("\n"))
    }
}

pub fn default_root() -> Result<PathBuf> {
    Ok(crate::config::dir()
        .context("no config directory for extensions")?
        .join("extensions"))
}

pub fn install_default(zip: &Path) -> Result<Extension> {
    install(zip, &default_root()?)
}

pub fn install(zip_path: &Path, root: &Path) -> Result<Extension> {
    let file = File::open(zip_path).with_context(|| format!("open {}", zip_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("read zip {}", zip_path.display()))?;
    if archive.len() > MAX_FILES {
        bail!("extension has too many files ({})", archive.len());
    }
    let manifest_text = {
        let mut entry = archive
            .by_name("package.json")
            .context("extension has no package.json at its root")?;
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        text
    };
    let manifest: Manifest =
        serde_json::from_str(&manifest_text).context("parse extension package.json")?;
    let id = manifest.name.trim();
    if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\']) || id.contains("..") {
        bail!("invalid extension name '{}'", manifest.name);
    }
    std::fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let target = root.join(id);
    if target.exists() {
        bail!("extension '{id}' is already installed");
    }
    let temp = root.join(format!(".install-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp)?;
    let result = extract(&mut archive, &temp);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    std::fs::rename(&temp, &target)
        .with_context(|| format!("install into {}", target.display()))?;
    load(&target)
}

fn extract(archive: &mut zip::ZipArchive<File>, dest: &Path) -> Result<()> {
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            bail!("unsafe path in extension: {:?}", entry.name());
        };
        if entry.is_dir() {
            std::fs::create_dir_all(dest.join(rel))?;
            continue;
        }
        let size = entry.size();
        if size > MAX_FILE_BYTES {
            bail!("extension file too large: {:?}", entry.name());
        }
        total += size;
        if total > MAX_TOTAL_BYTES {
            bail!(
                "extension exceeds the {} MiB limit",
                MAX_TOTAL_BYTES / 1024 / 1024
            );
        }
        let path = dest.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut bytes)?;
        let mut f = File::create(&path)?;
        f.write_all(&bytes)?;
    }
    Ok(())
}

pub fn load(dir: &Path) -> Result<Extension> {
    let manifest_path = dir.join("package.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&text).context("parse extension package.json")?;
    if manifest.name.trim().is_empty() {
        bail!("extension package.json has no name");
    }
    let files = |list: &[String]| -> Result<Vec<PathBuf>> {
        list.iter()
            .map(|rel| {
                let path = dir.join(rel);
                if !path.starts_with(dir) {
                    bail!("extension path escapes package: {rel}");
                }
                if !path.is_file() {
                    bail!("extension file missing: {rel}");
                }
                Ok(path)
            })
            .collect()
    };
    let scripts = files(&manifest.contributes.scripts)?;
    let themes = files(&manifest.contributes.themes)?;
    Ok(Extension {
        manifest,
        scripts,
        themes,
    })
}

pub fn get(root: &Path, id: &str) -> Result<Extension> {
    if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\']) || id.contains("..") {
        bail!("invalid extension name '{id}'");
    }
    let dir = root.join(id);
    if !dir.is_dir() {
        bail!("extension '{id}' is not installed");
    }
    load(&dir)
}

pub fn list(root: &Path) -> Result<Vec<Extension>> {
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(root) else {
        return Ok(out);
    };
    for entry in read.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if let Ok(ext) = load(&entry.path()) {
            out.push(ext);
        }
    }
    out.sort_by(|a, b| a.id().cmp(b.id()));
    Ok(out)
}

pub fn remove(root: &Path, id: &str) -> Result<()> {
    if id.is_empty() || id.starts_with('.') || id.contains(['/', '\\']) || id.contains("..") {
        bail!("invalid extension name '{id}'");
    }
    let dir = root.join(id);
    if !dir.is_dir() {
        bail!("extension '{id}' is not installed");
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(path: &Path, files: &[(&str, &str)]) {
        let f = File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in files {
            z.start_file(name, opts).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    fn root(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vixel-ext-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const MANIFEST: &str = r#"{
        "name": "demo",
        "displayName": "Demo Pack",
        "version": "1.0.0",
        "author": {"name": "Tester"},
        "contributes": {"scripts": ["main.lua"], "themes": ["theme.json"]}
    }"#;

    #[test]
    fn installs_lists_and_removes_a_package() {
        let dir = root("install");
        let zip = dir.join("demo.aseprite-extension");
        zip_with(
            &zip,
            &[
                ("package.json", MANIFEST),
                ("main.lua", "print('hello from demo')"),
                ("theme.json", r#"{"name":"demo"}"#),
            ],
        );
        let ext = install(&zip, &dir.join("ext")).unwrap();
        assert_eq!(ext.id(), "demo");
        assert_eq!(ext.label(), "Demo Pack");
        assert_eq!(ext.author(), "Tester");
        assert_eq!(ext.scripts.len(), 1);
        assert_eq!(ext.themes.len(), 1);
        let all = list(&dir.join("ext")).unwrap();
        assert_eq!(all.len(), 1);
        remove(&dir.join("ext"), "demo").unwrap();
        assert!(list(&dir.join("ext")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_contributed_scripts_against_the_editor() {
        let dir = root("run");
        let zip = dir.join("paint.aseprite-extension");
        zip_with(
            &zip,
            &[
                (
                    "package.json",
                    r#"{"name":"paint","contributes":{"scripts":["paint.lua"]}}"#,
                ),
                (
                    "paint.lua",
                    "app.activeSprite:setPixel(1, 1, {r=1,g=2,b=3,a=255})",
                ),
            ],
        );
        let ext = install(&zip, &dir.join("ext")).unwrap();
        let mut e = Editor::new(3, 3);
        ext.run_scripts(&mut e).unwrap();
        assert_eq!(e.sprite.get(0, 0, 1, 1), crate::color::Rgba::rgb(1, 2, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_traversal_and_missing_manifests() {
        let dir = root("bad");
        let bad = dir.join("bad.aseprite-extension");
        zip_with(
            &bad,
            &[
                ("package.json", r#"{"name":"bad"}"#),
                ("../escape.txt", "nope"),
            ],
        );
        assert!(install(&bad, &dir.join("ext")).is_err());
        assert!(!dir.join("escape.txt").exists());

        let empty = dir.join("empty.aseprite-extension");
        zip_with(&empty, &[("readme.txt", "no manifest")]);
        assert!(install(&empty, &dir.join("ext")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
