import json
import re
from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


# Keep the fork isolated from upstream while removing PTAP naming.
package = json.loads(read("package.json"))
package["version"] = "0.5.0-auto.1"
write("package.json", json.dumps(package, ensure_ascii=False, indent=2) + "\n")

cargo = read("src-tauri/Cargo.toml")
cargo = replace_once(
    cargo,
    'version = "0.5.0-ptap.1"',
    'version = "0.5.0-auto.1"',
    "Cargo.toml version",
)
write("src-tauri/Cargo.toml", cargo)

lock = read("src-tauri/Cargo.lock")
lock = replace_once(
    lock,
    'name = "tockyvoice"\nversion = "0.5.0-ptap.1"',
    'name = "tockyvoice"\nversion = "0.5.0-auto.1"',
    "Cargo.lock package version",
)
write("src-tauri/Cargo.lock", lock)

conf_path = Path("src-tauri/tauri.conf.json")
conf = json.loads(conf_path.read_text(encoding="utf-8"))
conf["productName"] = "Tocky Voice Automotive"
conf["version"] = "0.5.0-auto.1"
conf["identifier"] = "io.github.hoangquangthanhxd.tockyvoice.automotive"
for window in conf["app"]["windows"]:
    if window.get("label") == "main":
        window["title"] = "Tocky Voice Automotive"
conf_path.write_text(json.dumps(conf, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

# No PTAP/PTAP-NEXT dataset is bundled in the standalone application.
seed = Path("src-tauri/src/ptap_terminology_seed.json")
if seed.exists():
    seed.unlink()

defaults_path = Path("src-tauri/src/settings/defaults.rs")
defaults = defaults_path.read_text(encoding="utf-8")
defaults, count = re.subn(
    r"\n#\[cfg\(test\)\]\nmod terminology_seed_tests \{.*?\n\}\n\n/// Factory hotkeys",
    "\n/// Factory hotkeys",
    defaults,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"defaults terminology seed test removal: {count}")
defaults, count = re.subn(
    r"pub fn default_terminology_settings\(\) -> TerminologySettings \{.*?\n\}",
    "pub fn default_terminology_settings() -> TerminologySettings {\n    TerminologySettings::default()\n}",
    defaults,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"default terminology replacement: {count}")
defaults_path.write_text(defaults, encoding="utf-8")

settings_path = Path("src-tauri/src/settings/mod.rs")
settings = settings_path.read_text(encoding="utf-8")
settings = settings.replace(
    "/// User-created terms default above bundled seed terms, so they win provider caps.",
    "/// Higher values win provider caps and equal-length alias conflicts.",
)
settings = settings.replace(
    '#[serde(default = "defaults::default_terminology_settings")]\n    pub terminology: TerminologySettings,',
    "#[serde(default)]\n    pub terminology: TerminologySettings,",
)
settings = settings.replace(
    "fn settings_from_before_terminology_receive_the_ptap_seed() {",
    "fn settings_from_before_terminology_receive_an_empty_dictionary() {",
)
settings = settings.replace(
    "assert_eq!(parsed.terminology.entries.len(), 100);",
    "assert!(parsed.terminology.entries.is_empty());",
)
settings_path.write_text(settings, encoding="utf-8")

terminology_path = Path("src-tauri/src/terminology.rs")
terminology = terminology_path.read_text(encoding="utf-8")
terminology = replace_once(
    terminology,
    "fn maps_common_ptap_domain_aliases() {",
    "fn maps_common_automotive_domain_aliases() {",
    "terminology test name",
)
terminology_path.write_text(terminology, encoding="utf-8")

styles_path = Path("src/styles.css")
styles = styles_path.read_text(encoding="utf-8")
styles = replace_once(
    styles,
    "/* PTAP terminology editor */",
    "/* Automotive terminology editor */",
    "terminology CSS label",
)
styles_path.write_text(styles, encoding="utf-8")

editor_path = Path("src/components/terminology-editor.tsx")
editor = editor_path.read_text(encoding="utf-8")
editor, count = re.subn(
    r"\nconst SOURCE_LABELS: Record<string, string> = \{.*?\n\};\n",
    "\n",
    editor,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"SOURCE_LABELS removal: {count}")

new_preview = '''function isWordChar(value: string | undefined) {
  return Boolean(value && /[\\p{L}\\p{N}_]/u.test(value));
}

function previewMapping(text: string, entries: TerminologyEntry[]) {
  const candidates = entries
    .filter((entry) => entry.enabled && entry.canonical.trim())
    .flatMap((entry) =>
      [entry.canonical, ...entry.aliases]
        .map((alias) => alias.trim())
        .filter(Boolean)
        .map((alias) => ({
          canonical: entry.canonical.trim(),
          units: Array.from(alias).map((unit) => unit.toLocaleLowerCase()),
          priority: entry.priority,
        })),
    )
    .sort((a, b) => b.units.length - a.units.length || b.priority - a.priority);

  const chars = Array.from(text);
  const lowered = chars.map((unit) => unit.toLocaleLowerCase());
  let output = "";
  let index = 0;

  while (index < chars.length) {
    const candidate = candidates.find((item) => {
      const end = index + item.units.length;
      if (end > chars.length) return false;
      if (isWordChar(chars[index - 1]) || isWordChar(chars[end])) return false;
      return item.units.every((unit, offset) => lowered[index + offset] === unit);
    });

    if (candidate) {
      output += candidate.canonical;
      index += candidate.units.length;
    } else {
      output += chars[index];
      index += 1;
    }
  }

  return output;
}'''
editor, count = re.subn(
    r"function escapeRegExp\(value: string\) \{.*?\n\}\n\nfunction previewMapping\(text: string, entries: TerminologyEntry\[\]\) \{.*?\n\}",
    lambda _: new_preview,
    editor,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"preview mapping replacement: {count}")
editor = editor.replace(
    "{entry.source ? SOURCE_LABELS[entry.source] ?? entry.source : t.terminology.customSource}",
    "{entry.source ? `Imported · ${entry.source}` : t.terminology.customSource}",
)
editor_path.write_text(editor, encoding="utf-8")

i18n_path = Path("src/lib/i18n.ts")
i18n = i18n_path.read_text(encoding="utf-8")
replacements = {
    'title: "PTAP terminology"': 'title: "Automotive terminology"',
    'lede: "Automotive vocabulary from PTAP plus your own corrections. Exact aliases are mapped deterministically, while selected canonical terms are also sent to the speech provider as recognition hints."': 'lede: "Your automotive vocabulary and corrections. Exact aliases are mapped deterministically, while selected canonical terms are also sent to the speech provider as recognition hints."',
    'terminology: "Từ điển PTAP"': 'terminology: "Từ điển ô tô"',
    'title: "Từ điển PTAP"': 'title: "Từ điển ô tô"',
    'lede: "Thuật ngữ ô tô lấy từ dữ liệu PTAP và các sửa lỗi anh tự thêm. Alias được map xác định tại máy; các từ chuẩn được chọn còn được gửi cho STT để tăng khả năng nhận đúng ngay từ đầu."': 'lede: "Từ chuyên ngành ô tô và các cách nhận sai do anh tự quản lý. Alias được map xác định tại máy; các từ chuẩn được chọn còn được gửi cho STT để tăng khả năng nhận đúng ngay từ đầu."',
}
for old, new in replacements.items():
    if old not in i18n:
        raise SystemExit(f"i18n text not found: {old}")
    i18n = i18n.replace(old, new)
i18n_path.write_text(i18n, encoding="utf-8")

app_path = Path("src/settings-app.tsx")
app = app_path.read_text(encoding="utf-8")
app = app.replace(
    "getCurrentWindow().setTitle(`Tocky Voice v${v}`)",
    "getCurrentWindow().setTitle(`Tocky Voice Automotive v${v}`)",
)
app = app.replace(
    '<div className="rail__name">Tocky Voice</div>',
    '<div className="rail__name">Tocky Voice Automotive</div>',
)
app_path.write_text(app, encoding="utf-8")

old_workflow = Path(".github/workflows/ptap-windows-test.yml")
new_workflow = Path(".github/workflows/automotive-windows-test.yml")
if old_workflow.exists():
    workflow = old_workflow.read_text(encoding="utf-8")
    workflow = workflow.replace("PTAP", "Automotive").replace("ptap", "automotive")
    workflow = workflow.replace("0.5.0-automotive.1", "0.5.0-auto.1")
    workflow = workflow.replace(
        "feature/automotive-terminology-seed",
        "feature/automotive-terminology",
    )
    new_workflow.write_text(workflow, encoding="utf-8")
    old_workflow.unlink()

# Remove the one-shot migration files before the hard gate/commit.
Path(".github/workflows/neutralize-automotive.yml").unlink(missing_ok=True)
Path(".github/scripts/neutralize_automotive.py").unlink(missing_ok=True)

leftovers = []
for path in Path(".").rglob("*"):
    if not path.is_file() or ".git" in path.parts:
        continue
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue
    if "PTAP" in text or "ptap" in text:
        leftovers.append(str(path))
if leftovers:
    raise SystemExit("PTAP references remain: " + ", ".join(leftovers))
