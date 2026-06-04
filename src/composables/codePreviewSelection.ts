const CODE_PREVIEW_SURFACE_SELECTOR = ".code-preview-surface";
const CODE_LINE_ROW_SELECTOR = ".atv-line, .preview-line, .diff-line, .diff-sbs-row";
const CODE_LINE_NUMBER_SELECTOR = ".atv-ln, .preview-ln, .diff-ln";

export interface CodePreviewSelectionMeta {
  filePath?: string;
  language?: string;
  /** 1-based line number of the first visible line in the snippet */
  lineOffset?: number;
}

export interface CodePreviewSelectionContext {
  text: string;
  lineRange: { start: number; end: number } | null;
}

export function findCodePreviewSurface(element: Element | null): HTMLElement | null {
  if (!element) return null;
  const surface = element.closest(CODE_PREVIEW_SURFACE_SELECTOR);
  return surface instanceof HTMLElement ? surface : null;
}

function rangeIntersectsElement(range: Range, element: Element): boolean {
  if (typeof range.intersectsNode === "function") {
    try {
      return range.intersectsNode(element);
    } catch {
      // Fallback when intersectsNode throws on partial trees.
    }
  }
  const rangeRect = range.getBoundingClientRect();
  const elRect = element.getBoundingClientRect();
  if (rangeRect.width === 0 && rangeRect.height === 0) {
    return element.contains(range.startContainer) || element.contains(range.endContainer);
  }
  return !(
    elRect.right < rangeRect.left
    || elRect.left > rangeRect.right
    || elRect.bottom < rangeRect.top
    || elRect.top > rangeRect.bottom
  );
}

export function getCodePreviewSelectionContext(
  surface: HTMLElement,
): CodePreviewSelectionContext | null {
  const selection = document.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  if (!surface.contains(range.commonAncestorContainer)) return null;

  const text = selection.toString().replace(/\r\n/g, "\n");
  if (!text.trim()) return null;

  let start: number | null = null;
  let end: number | null = null;
  for (const row of surface.querySelectorAll(CODE_LINE_ROW_SELECTOR)) {
    if (!rangeIntersectsElement(range, row)) continue;
    const cells = row.classList.contains("diff-sbs-row")
      ? row.querySelectorAll(".diff-sbs-cell")
      : [row];
    for (const cell of cells) {
      if (!rangeIntersectsElement(range, cell)) continue;
      const ln = cell.querySelector(CODE_LINE_NUMBER_SELECTOR)?.textContent?.trim() ?? "";
      const lineNo = Number.parseInt(ln, 10);
      if (!Number.isFinite(lineNo)) continue;
      if (start === null) start = lineNo;
      end = lineNo;
    }
  }

  return {
    text,
    lineRange: start != null && end != null ? { start, end } : null,
  };
}

export function formatCodeSelectionForComposer(
  context: CodePreviewSelectionContext,
  meta: CodePreviewSelectionMeta,
): string {
  const { text, lineRange } = context;
  const offset = meta.lineOffset ?? 1;
  const absoluteRange = lineRange
    ? {
        start: lineRange.start + offset - 1,
        end: lineRange.end + offset - 1,
      }
    : null;

  const lang = meta.language?.trim();
  const fenceLang = lang && lang !== "text" ? lang : "";
  const location = meta.filePath
    ? absoluteRange
      ? `${meta.filePath}:${absoluteRange.start}-${absoluteRange.end}`
      : meta.filePath
    : absoluteRange
      ? `lines ${absoluteRange.start}-${absoluteRange.end}`
      : "";

  const header = location ? `${location}\n\n` : "";
  return `${header}\`\`\`${fenceLang}\n${text}\n\`\`\``;
}
