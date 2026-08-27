from pathlib import Path

ROOT = Path.cwd()


def replace_once(path: str, old: str, new: str):
    file = ROOT / path
    text = file.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing expected anchor in {path}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


# Navigation + screen wiring.
replace_once(
    "src/settings-app.tsx",
    'import { ProvidersEditor } from "./components/providers-editor";\n',
    'import { ProvidersEditor } from "./components/providers-editor";\nimport { TerminologyEditor } from "./components/terminology-editor";\n',
)
replace_once(
    "src/settings-app.tsx",
    '  InfoIcon,\n  KeyIcon,',
    '  BookIcon,\n  InfoIcon,\n  KeyIcon,',
)
replace_once(
    "src/settings-app.tsx",
    '  { id: "providers", key: "providers", Icon: PlugIcon },\n  { id: "behaviour", key: "hotkeys", Icon: KeyIcon },',
    '  { id: "providers", key: "providers", Icon: PlugIcon },\n  { id: "terminology", key: "terminology", Icon: BookIcon },\n  { id: "behaviour", key: "hotkeys", Icon: KeyIcon },',
)
replace_once(
    "src/settings-app.tsx",
    '          {section === "providers" && (\n            <ProvidersEditor settings={settings} onSettingsChange={update} />\n          )}\n          {section === "behaviour" && (',
    '          {section === "providers" && (\n            <ProvidersEditor settings={settings} onSettingsChange={update} />\n          )}\n          {section === "terminology" && (\n            <TerminologyEditor settings={settings} onSettingsChange={update} />\n          )}\n          {section === "behaviour" && (',
)

# Small hand-drawn dictionary icon, matching the existing icon set.
replace_once(
    "src/components/icons.tsx",
    'export const KeyIcon = ({ className }: Props) => (',
    '''export const BookIcon = ({ className }: Props) => (\n  <svg {...base} className={className} aria-hidden="true">\n    <path d="M2.5 2.5h4.25A2.25 2.25 0 0 1 9 4.75v8.5a2.25 2.25 0 0 0-2.25-2.25H2.5z" />\n    <path d="M13.5 2.5H9.25A2.25 2.25 0 0 0 7 4.75v8.5A2.25 2.25 0 0 1 9.25 11h4.25z" />\n  </svg>\n);\n\nexport const KeyIcon = ({ className }: Props) => (''',
)

# Typed i18n: both dictionaries must receive the same keys.
replace_once(
    "src/lib/i18n.ts",
    '    providers: "Providers",\n',
    '    providers: "Providers",\n    terminology: "Terminology",\n',
)
replace_once(
    "src/lib/i18n.ts",
    '    providers: "Nhà cung cấp",\n',
    '    providers: "Nhà cung cấp",\n    terminology: "Từ điển PTAP",\n',
)

en_block = '''  terminology: {\n    title: "PTAP terminology",\n    lede: "Automotive vocabulary from PTAP plus your own corrections. Exact aliases are mapped deterministically, while selected canonical terms are also sent to the speech provider as recognition hints.",\n    behaviourSection: "Behaviour",\n    enabled: "Use terminology dictionary",\n    enabledHint: "Turns deterministic alias mapping on or off without deleting any entries.",\n    sendToStt: "Send vocabulary to speech recognition",\n    sendToSttHint: "Prioritises selected canonical terms in Soniox or Deepgram before local mapping runs.",\n    total: "entries",\n    active: "enabled",\n    custom: "custom",\n    conflicts: "alias conflicts",\n    addSection: "Add a term",\n    canonical: "Canonical form",\n    canonicalPlaceholder: "e.g. 7PK2604",\n    aliases: "Spoken / commonly misheard forms",\n    aliasesPlaceholder: "One form per line",\n    aliasesHint: "Use exact forms you actually say or forms the STT commonly returns. One per line.",\n    add: "Add term",\n    listSection: "Dictionary",\n    search: "Search term, alias or source…",\n    customSource: "Custom",\n    providerHint: "Use as speech-recognition hint",\n    conflict: "Alias also belongs to another term",\n    noResults: "No matching terms.",\n    testSection: "Test mapping",\n    testHint: "This preview applies the same exact-alias idea locally. It does not call the speech provider.",\n    rawInput: "Raw transcript",\n    mappedOutput: "After terminology mapping",\n  },\n\n'''
vi_block = '''  terminology: {\n    title: "Từ điển PTAP",\n    lede: "Thuật ngữ ô tô lấy từ dữ liệu PTAP và các sửa lỗi anh tự thêm. Alias được map xác định tại máy; các từ chuẩn được chọn còn được gửi cho STT để tăng khả năng nhận đúng ngay từ đầu.",\n    behaviourSection: "Cách hoạt động",\n    enabled: "Dùng từ điển chuyên ngành",\n    enabledHint: "Bật/tắt map thuật ngữ mà không xóa dữ liệu đã lưu.",\n    sendToStt: "Gửi từ chuẩn cho STT",\n    sendToSttHint: "Ưu tiên các thuật ngữ đã chọn trong Soniox hoặc Deepgram trước khi map lại tại máy.",\n    total: "mục",\n    active: "đang bật",\n    custom: "tự thêm",\n    conflicts: "alias bị trùng",\n    addSection: "Thêm thuật ngữ",\n    canonical: "Từ chuẩn",\n    canonicalPlaceholder: "ví dụ 7PK2604",\n    aliases: "Cách đọc / cách thường nhận sai",\n    aliasesPlaceholder: "Mỗi cách một dòng",\n    aliasesHint: "Nhập đúng cách anh thường đọc hoặc dạng STT thường nhận sai. Mỗi cách một dòng.",\n    add: "Thêm từ",\n    listSection: "Danh sách từ điển",\n    search: "Tìm từ, alias hoặc nguồn…",\n    customSource: "Tự thêm",\n    providerHint: "Dùng làm gợi ý cho STT",\n    conflict: "Alias này cũng thuộc từ khác",\n    noResults: "Không có thuật ngữ phù hợp.",\n    testSection: "Thử map",\n    testHint: "Phần này mô phỏng map alias ngay trên máy, không gọi dịch vụ STT.",\n    rawInput: "Văn bản STT ban đầu",\n    mappedOutput: "Sau khi map từ điển",\n  },\n\n'''
replace_once(
    "src/lib/i18n.ts",
    '  behaviour: {\n    title: "Settings",',
    en_block + '  behaviour: {\n    title: "Settings",',
)
replace_once(
    "src/lib/i18n.ts",
    '  behaviour: {\n    title: "Cài đặt",',
    vi_block + '  behaviour: {\n    title: "Cài đặt",',
)

# Dedicated layout, appended so no existing selector semantics are changed.
styles = ROOT / "src/styles.css"
css = styles.read_text(encoding="utf-8")
marker = "/* PTAP terminology editor */"
if marker in css:
    raise RuntimeError("terminology styles already present")
css += r'''

/* PTAP terminology editor */
.term-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 18px;
  margin-top: 14px;
  font-size: 12px;
  color: var(--muted);
}
.term-stat--warn, .term-conflict { color: var(--danger, #b42318); }
.term-add-grid, .term-test-grid { display: grid; grid-template-columns: 1fr 1.4fr; gap: 14px; margin-top: 14px; }
.term-add-grid label, .term-test-grid label, .term-aliases { display: grid; gap: 7px; }
.term-add-grid input, .term-add-grid textarea, .term-test-grid textarea, .term-card textarea, .term-canonical, .term-search { width: 100%; box-sizing: border-box; }
.term-add-actions { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-top: 12px; }
.term-list-head { display: flex; align-items: end; justify-content: space-between; gap: 18px; margin-bottom: 12px; }
.term-search { max-width: 290px; }
.term-list { display: grid; gap: 10px; }
.term-card { border: 1px solid var(--line); border-radius: 10px; padding: 12px; background: var(--surface); transition: opacity 120ms ease; }
.term-card--off { opacity: .58; }
.term-card__top, .term-card__identity, .term-card__actions, .term-card__footer { display: flex; align-items: center; gap: 10px; }
.term-card__top { justify-content: space-between; }
.term-card__identity { min-width: 0; flex: 1; }
.term-card__actions { flex: none; }
.term-canonical { min-width: 120px; font-weight: 650; }
.term-aliases { margin-top: 10px; }
.term-card__footer { justify-content: space-between; margin-top: 10px; min-height: 22px; }
.term-provider-hint { display: inline-flex; align-items: center; gap: 7px; font-size: 12px; color: var(--muted); }
.term-conflict { font-size: 11.5px; text-align: right; }
.term-empty { padding: 24px 0; text-align: center; color: var(--muted); }
@media (max-width: 760px) {
  .term-add-grid, .term-test-grid { grid-template-columns: 1fr; }
  .term-list-head, .term-card__top, .term-card__footer { align-items: stretch; flex-direction: column; }
  .term-search { max-width: none; }
  .term-card__actions { justify-content: space-between; }
  .term-card__identity { align-items: stretch; flex-direction: column; }
  .term-conflict { text-align: left; }
}
'''
styles.write_text(css, encoding="utf-8")

print("terminology UI integration applied")
