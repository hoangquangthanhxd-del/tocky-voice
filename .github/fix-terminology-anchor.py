from pathlib import Path
p = Path('src-tauri/src/settings/mod.rs')
text = p.read_text(encoding='utf-8')
old = '''    pub aliases: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}'''
new = '''    pub aliases: Vec<String>,
    /// Lets users temporarily disable a mapping without deleting it.
    #[serde(default = "default_true")]
    pub enabled: bool,
}'''
if old not in text:
    raise RuntimeError('terminology entry anchor not found')
p.write_text(text.replace(old, new, 1), encoding='utf-8')
