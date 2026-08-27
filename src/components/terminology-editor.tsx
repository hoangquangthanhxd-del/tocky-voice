import { useMemo, useState } from "react";
import { useT } from "../lib/i18n";
import type { AppSettings, TerminologyEntry } from "../lib/types";
import { Switch } from "./providers-editor";

interface Props {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
}


function normalized(value: string) {
  return value.trim().toLocaleLowerCase();
}

function parseAliases(value: string) {
  return [...new Set(value.split("\n").map((item) => item.trim()).filter(Boolean))];
}

function isWordChar(value: string | undefined) {
  return Boolean(value && /[\p{L}\p{N}_]/u.test(value));
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
}

export function TerminologyEditor({ settings, onSettingsChange }: Props) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [newCanonical, setNewCanonical] = useState("");
  const [newAliases, setNewAliases] = useState("");
  const [testInput, setTestInput] = useState("");

  const entries = settings.terminology.entries;

  const aliasOwners = useMemo(() => {
    const owners = new Map<string, Set<string>>();
    for (const entry of entries) {
      for (const alias of entry.aliases) {
        const key = normalized(alias);
        if (!key) continue;
        const set = owners.get(key) ?? new Set<string>();
        set.add(entry.canonical);
        owners.set(key, set);
      }
    }
    return owners;
  }, [entries]);

  const filtered = useMemo(() => {
    const needle = normalized(query);
    if (!needle) return entries;
    return entries.filter((entry) =>
      [entry.canonical, entry.source ?? "", ...entry.aliases]
        .some((value) => normalized(value).includes(needle)),
    );
  }, [entries, query]);

  const patchEntry = (index: number, patch: Partial<TerminologyEntry>) => {
    const next = entries.map((entry, current) =>
      current === index ? { ...entry, ...patch } : entry,
    );
    onSettingsChange({
      ...settings,
      terminology: { ...settings.terminology, entries: next },
    });
  };

  const removeEntry = (index: number) => {
    onSettingsChange({
      ...settings,
      terminology: {
        ...settings.terminology,
        entries: entries.filter((_, current) => current !== index),
      },
    });
  };

  const addEntry = () => {
    const canonical = newCanonical.trim();
    if (!canonical) return;
    const key = normalized(canonical);
    if (entries.some((entry) => normalized(entry.canonical) === key)) return;

    const entry: TerminologyEntry = {
      canonical,
      aliases: parseAliases(newAliases),
      enabled: true,
      priority: 2_000,
      source: null,
      provider_hint: true,
    };

    onSettingsChange({
      ...settings,
      terminology: {
        ...settings.terminology,
        entries: [entry, ...entries],
      },
    });
    setNewCanonical("");
    setNewAliases("");
  };

  const customCount = entries.filter((entry) => !entry.source).length;
  const enabledCount = entries.filter((entry) => entry.enabled).length;
  const conflictCount = [...aliasOwners.values()].filter((owners) => owners.size > 1).length;
  const mappedPreview = settings.terminology.enabled
    ? previewMapping(testInput, entries)
    : testInput;

  return (
    <>
      <h1 className="view__title">{t.terminology.title}</h1>
      <p className="view__lede">{t.terminology.lede}</p>

      <section className="section">
        <h2 className="section__title">{t.terminology.behaviourSection}</h2>
        <div className="row">
          <div>
            <div className="row__label">{t.terminology.enabled}</div>
            <span className="row__hint">{t.terminology.enabledHint}</span>
          </div>
          <div className="row__control">
            <Switch
              checked={settings.terminology.enabled}
              onChange={(enabled) =>
                onSettingsChange({
                  ...settings,
                  terminology: { ...settings.terminology, enabled },
                })
              }
            />
          </div>
        </div>
        <div className="row">
          <div>
            <div className="row__label">{t.terminology.sendToStt}</div>
            <span className="row__hint">{t.terminology.sendToSttHint}</span>
          </div>
          <div className="row__control">
            <Switch
              checked={settings.terminology.send_to_stt}
              onChange={(send_to_stt) =>
                onSettingsChange({
                  ...settings,
                  terminology: { ...settings.terminology, send_to_stt },
                })
              }
            />
          </div>
        </div>

        <div className="term-stats">
          <span><strong>{entries.length}</strong> {t.terminology.total}</span>
          <span><strong>{enabledCount}</strong> {t.terminology.active}</span>
          <span><strong>{customCount}</strong> {t.terminology.custom}</span>
          {conflictCount > 0 && (
            <span className="term-stat--warn"><strong>{conflictCount}</strong> {t.terminology.conflicts}</span>
          )}
        </div>
      </section>

      <section className="section">
        <h2 className="section__title">{t.terminology.addSection}</h2>
        <div className="term-add-grid">
          <label>
            <span className="row__label">{t.terminology.canonical}</span>
            <input
              value={newCanonical}
              onChange={(e) => setNewCanonical(e.target.value)}
              placeholder={t.terminology.canonicalPlaceholder}
            />
          </label>
          <label>
            <span className="row__label">{t.terminology.aliases}</span>
            <textarea
              value={newAliases}
              onChange={(e) => setNewAliases(e.target.value)}
              placeholder={t.terminology.aliasesPlaceholder}
              rows={3}
            />
          </label>
        </div>
        <div className="term-add-actions">
          <span className="row__hint">{t.terminology.aliasesHint}</span>
          <button onClick={addEntry} disabled={!newCanonical.trim()}>{t.terminology.add}</button>
        </div>
      </section>

      <section className="section">
        <div className="term-list-head">
          <div>
            <h2 className="section__title">{t.terminology.listSection}</h2>
            <span className="row__hint">{filtered.length} / {entries.length}</span>
          </div>
          <input
            className="term-search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t.terminology.search}
          />
        </div>

        <div className="term-list">
          {filtered.map((entry) => {
            const index = entries.indexOf(entry);
            const conflicts = entry.aliases.filter((alias) => (aliasOwners.get(normalized(alias))?.size ?? 0) > 1);
            return (
              <article key={`${entry.canonical}-${index}`} className={`term-card ${entry.enabled ? "" : "term-card--off"}`}>
                <div className="term-card__top">
                  <div className="term-card__identity">
                    <input
                      className="term-canonical"
                      value={entry.canonical}
                      onChange={(e) => patchEntry(index, { canonical: e.target.value })}
                      aria-label={t.terminology.canonical}
                    />
                    <span className={`chip ${entry.source ? "" : "chip--ok"}`}>
                      {entry.source ? `Imported · ${entry.source}` : t.terminology.customSource}
                    </span>
                  </div>
                  <div className="term-card__actions">
                    <Switch checked={entry.enabled} onChange={(enabled) => patchEntry(index, { enabled })} />
                    <button className="btn-quiet btn-danger" onClick={() => removeEntry(index)}>
                      {t.common.delete}
                    </button>
                  </div>
                </div>

                <label className="term-aliases">
                  <span className="row__label">{t.terminology.aliases}</span>
                  <textarea
                    rows={Math.min(5, Math.max(2, entry.aliases.length || 2))}
                    value={entry.aliases.join("\n")}
                    onChange={(e) => patchEntry(index, { aliases: parseAliases(e.target.value) })}
                    placeholder={t.terminology.aliasesPlaceholder}
                  />
                </label>

                <div className="term-card__footer">
                  <label className="term-provider-hint">
                    <input
                      type="checkbox"
                      checked={entry.provider_hint}
                      onChange={(e) => patchEntry(index, { provider_hint: e.target.checked })}
                    />
                    {t.terminology.providerHint}
                  </label>
                  {conflicts.length > 0 && (
                    <span className="term-conflict">{t.terminology.conflict}: {conflicts.join(", ")}</span>
                  )}
                </div>
              </article>
            );
          })}
          {filtered.length === 0 && <div className="term-empty">{t.terminology.noResults}</div>}
        </div>
      </section>

      <section className="section">
        <h2 className="section__title">{t.terminology.testSection}</h2>
        <p className="row__hint">{t.terminology.testHint}</p>
        <div className="term-test-grid">
          <label>
            <span className="row__label">{t.terminology.rawInput}</span>
            <textarea rows={4} value={testInput} onChange={(e) => setTestInput(e.target.value)} />
          </label>
          <label>
            <span className="row__label">{t.terminology.mappedOutput}</span>
            <textarea rows={4} value={mappedPreview} readOnly />
          </label>
        </div>
      </section>
    </>
  );
}
