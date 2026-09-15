/**
 * Hand-drawn on a 16px grid rather than pulled from an icon package — six glyphs do
 * not justify a dependency, and drawing them keeps the stroke weight matched to the
 * type rather than fighting it.
 */

type Props = { className?: string };

const base = {
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.4,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const MicIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <rect x="5.75" y="1.75" width="4.5" height="8" rx="2.25" />
    <path d="M3.25 7.25v.5a4.75 4.75 0 0 0 9.5 0v-.5M8 12.5v1.75" />
  </svg>
);

export const ModesIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M2.25 4.25h11.5M2.25 8h7.5M2.25 11.75h4.5" />
    <circle cx="12.25" cy="11.75" r="1.6" />
  </svg>
);

export const KeyIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <rect x="1.75" y="3.25" width="12.5" height="9.5" rx="1.8" />
    <path d="M4.5 6.25h.01M7 6.25h.01M9.5 6.25h.01M11.5 6.25h.01M5 9.5h6" />
  </svg>
);

export const PlugIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M6 1.75v3.5M10 1.75v3.5" />
    <path d="M3.75 5.25h8.5v2.5a4.25 4.25 0 0 1-8.5 0v-2.5ZM8 12v2.25" />
  </svg>
);

export const LogIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="6.25" />
    <path d="M8 4.5V8l2.5 1.5" />
  </svg>
);

export const InfoIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="6.25" />
    <path d="M8 7.25v4M8 4.9h.01" />
  </svg>
);

/** Small action glyphs for the compact dictation overlay. */
export const CloseIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="m4 4 8 8M12 4l-8 8" />
  </svg>
);

export const CollapseIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M3 8h10M9.5 4.5 13 8l-3.5 3.5" />
  </svg>
);

export const ExpandIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M3 8h10M6.5 4.5 3 8l3.5 3.5" />
  </svg>
);

export const SettingsIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="2.2" />
    <path d="M8 1.9v1.4M8 12.7v1.4M14.1 8h-1.4M3.3 8H1.9M12.3 3.7l-1 1M4.7 11.3l-1 1M12.3 12.3l-1-1M4.7 4.7l-1-1" />
  </svg>
);

export const ProcessIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M8 2.25v5.1l3.4 2.05" />
    <circle cx="8" cy="8" r="5.75" />
  </svg>
);

export const SendIcon = ({ className }: Props) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="m2.25 2.5 11.5 5.5-11.5 5.5 2.1-4.15H9.5V6.65H4.35L2.25 2.5Z" />
  </svg>
);

/**
 * Brand mark — the app icon at sidebar size, drawn as a line glyph.
 *
 * Redrawn rather than scaled: at 22px the icon's six bars a side close into a block and
 * its cradle thins to nothing, so the waveform is cut to two bars a side and each part
 * carries its own stroke weight. The capsule is a stroke too, not a filled shape, so the
 * whole mark stays one `currentColor` line drawing like the rest of the icon set.
 */
export const WaveMark = ({ className }: Props) => (
  <svg
    viewBox="0 0 22 22"
    fill="none"
    stroke="currentColor"
    strokeLinecap="round"
    className={className}
    aria-hidden="true"
  >
    <g strokeWidth="1.5">
      <path d="M2.2 9.4v3.2" />
      <path d="M5.4 7.2v7.6" />
      <path d="M16.6 7.2v7.6" />
      <path d="M19.8 9.4v3.2" />
    </g>
    {/* Capsule head: a bar thick enough to read as a body rather than another wave. */}
    <path d="M11 4.5v5.4" strokeWidth="3.4" />
    <path d="M8 10.3a3 3 0 0 0 6 0" strokeWidth="1.2" />
    <path d="M11 13.3v2.9" strokeWidth="1.2" />
    <path d="M8.8 17.2h4.4" strokeWidth="1.3" />
  </svg>
);
