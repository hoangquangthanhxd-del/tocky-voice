/**
 * The floating readout shown while dictating from another app.
 *
 * On a phone the panel starts as one quiet line, then expands only when someone
 * wants to correct or send the text. The same component remains useful on desktop:
 * it can be collapsed at any time, and its narrow edge remains a drag surface.
 */

import { useEffect, useRef, useState, type PointerEvent, type ReactElement } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./lib/api";
import { formatError } from "./lib/format-error";
import { useDictationEvents } from "./lib/use-dictation-events";
import { useT } from "./lib/i18n";
import {
  CloseIcon,
  CollapseIcon,
  ExpandIcon,
  ProcessIcon,
  SendIcon,
  SettingsIcon,
} from "./components/icons";

const isMobileDevice = () => /android|iphone|ipad|ipod/i.test(navigator.userAgent);

export function OverlayPanel() {
  const { phase, transcript, partial, error } = useDictationEvents();
  const t = useT();
  const [compact, setCompact] = useState(isMobileDevice);
  const [editableText, setEditableText] = useState("");
  const [edited, setEdited] = useState(false);
  const [copied, setCopied] = useState(false);
  const textRef = useRef<HTMLInputElement>(null);

  const incomingText = error
    ? formatError(error, t)
    : [transcript, partial].filter(Boolean).join(" ");
  const displayText = editableText;
  const recording = phase === "recording";

  // Recognition events remain authoritative until the user changes the field. Once
  // edited, preserve the correction even if a late event arrives from the provider.
  useEffect(() => {
    if (!edited) setEditableText(incomingText ?? "");
    setCopied(false);
  }, [incomingText, edited]);

  // A new take is a new editable result. It must not inherit the edited lock from the
  // last one, otherwise its live words would never reach the textbox.
  useEffect(() => {
    if (phase === "recording") setEdited(false);
  }, [phase]);

  // An input only horizontally scrolls to its caret while focused. The floating
  // readout must also reveal the newest words while the user is not touching it.
  useEffect(() => {
    const node = textRef.current;
    if (node) node.scrollLeft = node.scrollWidth;
  }, [displayText, compact]);

  const startDragging = (event: PointerEvent<HTMLDivElement>) => {
    if (
      event.button !== 0 ||
      (event.target as HTMLElement).closest("[data-overlay-control]")
    ) {
      return;
    }
    getCurrentWindow().startDragging().catch(() => undefined);
  };

  const close = () => {
    if (recording || phase !== "idle") api.cancelRecording().catch(() => undefined);
    getCurrentWindow().hide().catch(() => undefined);
  };

  const send = () => {
    if (!displayText.trim()) return;
    api
      .copyText(displayText)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1600);
      })
      .catch(() => undefined);
  };

  return (
    <div
      className={`overlay overlay--${compact ? "compact" : "expanded"}`}
      onPointerDown={startDragging}
    >
      {compact ? (
        <div className="overlay__compact-line">
          <span className={`overlay__dot overlay__dot--${phase}`} aria-hidden="true" />
          <span className={`overlay__compact-text ${error ? "overlay__error" : ""}`}>
            {displayText || t.overlay.speak}
          </span>
          <IconButton
            label={t.overlay.expand}
            onClick={() => setCompact(false)}
            Icon={ExpandIcon}
          />
        </div>
      ) : (
        <div className="overlay__expanded-line">
          {/* The field is intentionally a single line: it keeps the panel short and
              makes the newest character reachable without growing over other apps. */}
          <input
            ref={textRef}
            className={`overlay__result ${error ? "overlay__result--error" : ""}`}
            aria-label={t.overlay.resultLabel}
            value={displayText}
            placeholder={t.overlay.speak}
            onPointerDown={(event) => event.stopPropagation()}
            onChange={(event) => {
              setEdited(true);
              setEditableText(event.currentTarget.value);
            }}
          />
          <div className="overlay__actions" data-overlay-control>
            <IconButton label={t.overlay.close} onClick={close} Icon={CloseIcon} danger />
            <IconButton
              label={t.overlay.collapse}
              onClick={() => setCompact(true)}
              Icon={CollapseIcon}
            />
            <IconButton
              label={t.overlay.settings}
              onClick={() => api.showMainWindow().catch(() => undefined)}
              Icon={SettingsIcon}
            />
            <IconButton
              label={recording ? t.overlay.process : t.overlay.processUnavailable}
              onClick={() => api.stopRecording().catch(() => undefined)}
              Icon={ProcessIcon}
              disabled={!recording}
            />
            <IconButton
              label={copied ? t.overlay.copied : t.overlay.send}
              onClick={send}
              Icon={SendIcon}
              disabled={!displayText.trim()}
            />
          </div>
        </div>
      )}
    </div>
  );
}

type IconButtonProps = {
  label: string;
  onClick: () => void;
  Icon: (props: { className?: string }) => ReactElement;
  disabled?: boolean;
  danger?: boolean;
};

function IconButton({ label, onClick, Icon, disabled, danger }: IconButtonProps) {
  return (
    <button
      type="button"
      className={`overlay__icon-button ${danger ? "overlay__icon-button--danger" : ""}`}
      aria-label={label}
      title={label}
      data-overlay-control
      disabled={disabled}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={onClick}
    >
      <Icon className="overlay__icon" />
    </button>
  );
}
