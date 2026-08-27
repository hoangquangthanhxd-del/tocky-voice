/**
 * Main window shell.
 *
 * A left rail rather than top tabs: this is a settings-heavy desktop app, and the rail
 * keeps the active section visible while leaving the full width for content. It also
 * gives the window somewhere honest to show live state, so you can tell at a glance
 * whether the app is listening.
 *
 * Edits save automatically on a short debounce. Everything here is small and
 * reversible, and a hotkey change should take effect the moment you finish setting it.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./lib/api";
import { useDictationEvents } from "./lib/use-dictation-events";
import { useUpdateCheck } from "./lib/use-update-check";
import { useT } from "./lib/i18n";
import { OnboardingFlow } from "./onboarding/onboarding-flow";
import type { AppSettings, LlmPreset } from "./lib/types";
import { BehaviourEditor } from "./components/behaviour-editor";
import { DictationPanel } from "./components/dictation-panel";
import { HistoryList } from "./components/history-list";
import { ModesEditor } from "./components/modes-editor";
import { ProvidersEditor } from "./components/providers-editor";
import { TerminologyEditor } from "./components/terminology-editor";
import { AboutPanel } from "./components/about-panel";
import { UpdateBanner } from "./components/update-banner";
import {
  BookIcon,
  InfoIcon,
  KeyIcon,
  LogIcon,
  MicIcon,
  ModesIcon,
  PlugIcon,
  WaveMark,
} from "./components/icons";

const SECTIONS = [
  { id: "dictate", key: "dictate", Icon: MicIcon },
  { id: "modes", key: "modes", Icon: ModesIcon },
  { id: "providers", key: "providers", Icon: PlugIcon },
  { id: "terminology", key: "terminology", Icon: BookIcon },
  { id: "behaviour", key: "hotkeys", Icon: KeyIcon },
  { id: "history", key: "history", Icon: LogIcon },
  { id: "about", key: "about", Icon: InfoIcon },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

const SAVE_DEBOUNCE_MS = 400;
const SETTINGS_LOAD_ATTEMPTS = 20;
const SETTINGS_LOAD_RETRY_MS = 250;

async function getSettingsWithRetry(): Promise<AppSettings> {
  let lastError: unknown = new Error("settings unavailable");
  for (let attempt = 0; attempt < SETTINGS_LOAD_ATTEMPTS; attempt += 1) {
    try {
      return await api.getSettings();
    } catch (error) {
      lastError = error;
      if (attempt + 1 < SETTINGS_LOAD_ATTEMPTS) {
        await new Promise((resolve) => window.setTimeout(resolve, SETTINGS_LOAD_RETRY_MS));
      }
    }
  }
  throw lastError;
}

export function SettingsApp() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [section, setSection] = useState<SectionId>("dictate");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [presets, setPresets] = useState<LlmPreset[]>([]);
  const [version, setVersion] = useState<string | null>(null);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  const { phase } = useDictationEvents();
  const t = useT();
  // Never checks (and never shows a banner) until onboarding is done — a first-run
  // user should not see "a new version is out" mid-walkthrough.
  const updateCheck = useUpdateCheck(
    Boolean(settings?.auto_check_updates) && Boolean(settings?.onboarding_completed),
  );

  const saveTimer = useRef<number | undefined>(undefined);
  // Distinguishes local edits from settings pushed by the backend (e.g. a mode switched
  // by hotkey), so an inbound update is not echoed straight back.
  const dirty = useRef(false);

  const loadSettings = useCallback(() => {
    setLoadError(null);
    return getSettingsWithRetry()
      .then((next) => {
        setSettings(next);
        setLoadError(null);
      })
      .catch((error) => {
        console.error("failed to load settings", error);
        setLoadError(String(error));
      });
  }, []);

  useEffect(() => {
    getVersion().then((v) => {
      setVersion(v);
      // The rail already shows the version in small type; the title bar is the
      // spot people actually glance at, so it carries the version too.
      getCurrentWindow().setTitle(`Tocky Voice Automotive v${v}`).catch(() => undefined);
    }).catch(() => undefined);
    void loadSettings();
    api.listLlmPresets().then(setPresets).catch(() => undefined);
    const off = listen(api.EVENTS.settingsChanged, () => {
      if (dirty.current) return;
      void loadSettings();
    });
    return () => {
      off.then((fn) => fn()).catch(() => undefined);
    };
  }, [loadSettings]);

  const update = useCallback((next: AppSettings) => {
    dirty.current = true;
    setSettings(next);
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      api
        .saveSettings(next)
        .then(() => setSaveError(null))
        .catch((e) => setSaveError(String(e)))
        .finally(() => {
          dirty.current = false;
        });
    }, SAVE_DEBOUNCE_MS);
  }, []);

  if (!settings) {
    return (
      <div className="loading">
        {loadError ? (
          <>
            <div>Could not load settings: {loadError}</div>
            <button type="button" onClick={() => void loadSettings()}>
              Retry
            </button>
          </>
        ) : (
          t.nav.loading
        )}
      </div>
    );
  }

  // Shown over everything: on a fresh install none of the tabs behind it can do
  // anything useful until the walkthrough's steps are done anyway.
  if (!settings.onboarding_completed) {
    return (
      <OnboardingFlow
        settings={settings}
        onSettingsChange={update}
        onDone={() => setSection("dictate")}
      />
    );
  }

  const pip =
    phase === "recording" ? "live" : phase === "idle" ? "idle" : "busy";

  return (
    <div className="app">
      <nav className="rail">
        <div className="rail__brand">
          <WaveMark className="rail__mark" />
          <div>
            <div className="rail__name">Tocky Voice Automotive</div>
            <span className="rail__sub">{version ? `v${version}` : ""}</span>
          </div>
        </div>

        {SECTIONS.map(({ id, key, Icon }) => (
          <button
            key={id}
            className={`rail__item ${section === id ? "rail__item--on" : ""}`}
            onClick={() => setSection(id)}
          >
            <Icon className="rail__icon" />
            {t.nav[key]}
          </button>
        ))}

        <div className="rail__spacer" />

        <div className="rail__status">
          <span className={`rail__pip rail__pip--${pip}`} />
          {phase === "idle" ? t.nav.idle : t.phase[phase]}
        </div>
      </nav>

      <main className="view">
        <div className="view__inner">
          {saveError && <div className="notice notice--error">{saveError}</div>}
          {updateCheck.state === "available" && !bannerDismissed && updateCheck.update && (
            <UpdateBanner
              version={updateCheck.update.version}
              onSeeWhatsNew={() => setSection("about")}
              onDismiss={() => setBannerDismissed(true)}
            />
          )}

          {section === "dictate" && (
            <DictationPanel settings={settings} onSettingsChange={update} />
          )}
          {section === "modes" && (
            <ModesEditor settings={settings} onSettingsChange={update} />
          )}
          {section === "providers" && (
            <ProvidersEditor settings={settings} onSettingsChange={update} />
          )}
          {section === "terminology" && (
            <TerminologyEditor settings={settings} onSettingsChange={update} />
          )}
          {section === "behaviour" && (
            <BehaviourEditor settings={settings} onSettingsChange={update} />
          )}
          {section === "history" && <HistoryList />}
          {section === "about" && (
            <AboutPanel presets={presets} version={version} update={updateCheck} />
          )}
        </div>
      </main>
    </div>
  );
}
