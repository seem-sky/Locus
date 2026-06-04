import { reactive } from "vue";
import type {
  NotificationSoundMode,
  NotificationSoundSource,
} from "../services/notificationSounds";

export type FontSlot = "ui" | "prose" | "monoInline" | "monoBlock" | "monoEditor";

export interface CodePreviewTypography {
  /** Editor code preview font size in px */
  fontSize: number;
  /** Unitless line height multiplier */
  lineHeight: number;
  /** Letter spacing in em */
  letterSpacing: number;
}
export type DiffReviewTarget = "inline" | "window";
export type ChatDiffReviewTarget = DiffReviewTarget;
export type GitDiffReviewTarget = DiffReviewTarget;

export interface DisplaySettings {
  /** Auto-open TODO panel when todos arrive */
  todoAutoOpen: boolean;
  /** Auto-open file changes panel when changes arrive */
  changesAutoOpen: boolean;
  /** Auto-close file changes panel when a new round starts */
  changesAutoClose: boolean;
  /** Enable hover preview popovers for file changes */
  fileChangePopoverEnabled: boolean;
  /** Auto-show thinking process in chat (transcript + side panel) */
  showThinkingProcess: boolean;
  /** Auto-open thinking panel when thinking starts */
  thinkingAutoOpen: boolean;
  /** Auto-expand inline thinking content in the chat transcript */
  thinkingAutoExpand: boolean;
  /** Default target for reviewing chat file diffs */
  chatDiffReviewTarget: DiffReviewTarget;
  /** Default target for reviewing Git file diffs */
  gitDiffReviewTarget: DiffReviewTarget;
  /** Right-align user messages in the session transcript */
  rightAlignUserMessages: boolean;
  /** Collapse completed tool call batches in chat transcript */
  compactToolCalls: boolean;
  /** Hide completed thinking blocks in chat transcript */
  hideThinkingBlocks: boolean;
  /** Merge Git tree status letters into colored file icons */
  mergeGitTreeStatusIcon: boolean;
  /** Hide Git command suggestions in Git terminal */
  hideGitCommandSuggestions: boolean;
  /** Enable desktop notifications when the app is not focused */
  systemNotificationsEnabled: boolean;
  /** Notify when a chat run completes */
  notifyOnChatDone: boolean;
  /** Notify when a subagent run completes */
  notifyOnSubagentDone: boolean;
  /** Notify when the agent asks the user a question */
  notifyOnAskUser: boolean;
  /** Notify when a chat run fails */
  notifyOnChatError: boolean;
  /** Notify when tool approval is required */
  notifyOnToolConfirm: boolean;
  /** Enable sound alerts for key chat events */
  soundAlertsEnabled: boolean;
  /** Sound profile used for sound alerts */
  soundAlertMode: NotificationSoundMode;
  /** Sound source used for sound alerts */
  soundAlertSource: NotificationSoundSource;
  /** Custom sound file path used when soundAlertSource is custom */
  soundAlertCustomFilePath: string;
  /** Sound alert volume, stored as a percentage from 0 to 100 */
  soundAlertVolume: number;
  /** Play a sound when a chat run completes */
  soundOnChatDone: boolean;
  /** Play a sound when a subagent run completes */
  soundOnSubagentDone: boolean;
  /** Play a sound when the agent asks the user a question */
  soundOnAskUser: boolean;
  /** Play a sound when a chat run fails */
  soundOnChatError: boolean;
  /** Play a sound when tool approval is required */
  soundOnToolConfirm: boolean;
  /** Per-slot font-family overrides (empty string = use default) */
  fonts: Record<FontSlot, string>;
  /** Typography for asset preview, diff viewer, and file hover previews */
  codePreview: CodePreviewTypography;
}

const STORAGE_KEY = "locus-display-settings";

const defaultFonts: Record<FontSlot, string> = {
  ui: "",
  prose: "",
  monoInline: "",
  monoBlock: "",
  monoEditor: "",
};

const defaultCodePreview: CodePreviewTypography = {
  fontSize: 12,
  lineHeight: 1.5,
  letterSpacing: 0,
};

export function clampCodePreviewTypography(
  raw: Partial<CodePreviewTypography> | undefined,
): CodePreviewTypography {
  const fontSize = Number(raw?.fontSize);
  const lineHeight = Number(raw?.lineHeight);
  const letterSpacing = Number(raw?.letterSpacing);
  return {
    fontSize: Number.isFinite(fontSize)
      ? Math.min(24, Math.max(10, Math.round(fontSize)))
      : defaultCodePreview.fontSize,
    lineHeight: Number.isFinite(lineHeight)
      ? Math.min(2.5, Math.max(1, Math.round(lineHeight * 100) / 100))
      : defaultCodePreview.lineHeight,
    letterSpacing: Number.isFinite(letterSpacing)
      ? Math.min(0.2, Math.max(-0.05, Math.round(letterSpacing * 1000) / 1000))
      : defaultCodePreview.letterSpacing,
  };
}

const defaults: DisplaySettings = {
  todoAutoOpen: true,
  changesAutoOpen: true,
  changesAutoClose: true,
  fileChangePopoverEnabled: true,
  showThinkingProcess: false,
  thinkingAutoOpen: false,
  thinkingAutoExpand: true,
  chatDiffReviewTarget: "window",
  gitDiffReviewTarget: "window",
  rightAlignUserMessages: true,
  compactToolCalls: true,
  hideThinkingBlocks: true,
  mergeGitTreeStatusIcon: true,
  hideGitCommandSuggestions: false,
  systemNotificationsEnabled: true,
  notifyOnChatDone: true,
  notifyOnSubagentDone: false,
  notifyOnAskUser: true,
  notifyOnChatError: true,
  notifyOnToolConfirm: true,
  soundAlertsEnabled: false,
  soundAlertMode: "bright",
  soundAlertSource: "builtin",
  soundAlertCustomFilePath: "",
  soundAlertVolume: 50,
  soundOnChatDone: true,
  soundOnSubagentDone: false,
  soundOnAskUser: true,
  soundOnChatError: true,
  soundOnToolConfirm: true,
  fonts: { ...defaultFonts },
  codePreview: { ...defaultCodePreview },
};

function load(): DisplaySettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      const merged = {
        ...defaults,
        ...parsed,
        fonts: { ...defaultFonts, ...parsed.fonts },
        codePreview: clampCodePreviewTypography(parsed.codePreview),
      };
      if (parsed.showThinkingProcess === undefined) {
        merged.showThinkingProcess =
          parsed.hideThinkingBlocks === false || parsed.thinkingAutoOpen === true;
      }
      merged.hideThinkingBlocks = !merged.showThinkingProcess;
      merged.thinkingAutoOpen = merged.showThinkingProcess;
      return merged;
    }
  } catch { /* ignore */ }
  return { ...defaults, fonts: { ...defaultFonts }, codePreview: { ...defaultCodePreview } };
}

function save(s: DisplaySettings) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(s)); } catch { /* ignore */ }
}

const state = reactive<DisplaySettings>(load());

export function useDisplaySettings() {
  function set<K extends keyof DisplaySettings>(key: K, value: DisplaySettings[K]) {
    state[key] = value;
    save({ ...state });
  }

  function setFont(slot: FontSlot, value: string) {
    state.fonts[slot] = value;
    save({ ...state, fonts: { ...state.fonts } });
    applyFonts(state.fonts);
  }

  function setCodePreview<K extends keyof CodePreviewTypography>(
    key: K,
    value: CodePreviewTypography[K],
  ) {
    state.codePreview = clampCodePreviewTypography({ ...state.codePreview, [key]: value });
    save({ ...state });
    applyCodePreviewTypography(state.codePreview);
  }

  function resetCodePreview() {
    state.codePreview = { ...defaultCodePreview };
    save({ ...state });
    applyCodePreviewTypography(state.codePreview);
  }

  function setShowThinkingProcess(value: boolean) {
    state.showThinkingProcess = value;
    state.hideThinkingBlocks = !value;
    state.thinkingAutoOpen = value;
    save({ ...state });
  }

  return { state, set, setFont, setCodePreview, resetCodePreview, setShowThinkingProcess };
}

/* ---- Font CSS-variable application ---- */

const slotToCssVar: Record<FontSlot, string> = {
  ui: "--font-ui",
  prose: "--font-prose",
  monoInline: "--font-mono-inline",
  monoBlock: "--font-mono-block",
  monoEditor: "--font-mono-editor",
};

const slotToFallbackVar: Record<FontSlot, string> = {
  ui: "var(--font-stack-sans)",
  prose: "var(--font-stack-sans)",
  monoInline: "var(--font-stack-mono)",
  monoBlock: "var(--font-stack-mono)",
  monoEditor: "var(--font-stack-mono)",
};

/** Slots not exposed to UI but that should follow an exposed slot */
const aliasSlots: { cssVar: string; follows: FontSlot; fallback: string }[] = [
  { cssVar: "--font-mono-identifier", follows: "monoInline", fallback: "var(--font-stack-mono)" },
  { cssVar: "--font-mono-display",    follows: "monoEditor", fallback: "var(--font-stack-mono)" },
];

function applyFonts(fonts: Record<FontSlot, string>) {
  const root = document.documentElement;
  for (const slot of Object.keys(slotToCssVar) as FontSlot[]) {
    const custom = fonts[slot]?.trim();
    const cssVar = slotToCssVar[slot];
    if (custom) {
      root.style.setProperty(cssVar, `${custom}, ${slotToFallbackVar[slot]}`);
    } else {
      root.style.setProperty(cssVar, slotToFallbackVar[slot]);
    }
  }
  for (const alias of aliasSlots) {
    const custom = fonts[alias.follows]?.trim();
    if (custom) {
      root.style.setProperty(alias.cssVar, `${custom}, ${alias.fallback}`);
    } else {
      root.style.setProperty(alias.cssVar, alias.fallback);
    }
  }
}

function applyCodePreviewTypography(codePreview: CodePreviewTypography) {
  const root = document.documentElement;
  const cp = clampCodePreviewTypography(codePreview);
  root.style.setProperty("--code-preview-font-size", `${cp.fontSize}px`);
  root.style.setProperty("--code-preview-line-height", String(cp.lineHeight));
  root.style.setProperty("--code-preview-letter-spacing", `${cp.letterSpacing}em`);
}

/** Call once from App.vue to apply saved display typography on startup */
export function initFonts() {
  applyFonts(state.fonts);
  applyCodePreviewTypography(state.codePreview);
}
