import { useMemo } from "react";
import { DiffEditor } from "@monaco-editor/react";

interface Props {
  path: string;
  oldText: string;
  newText: string;
  maxHeight?: number;
}

function lineDiffSummary(oldText: string, newText: string): { added: number; removed: number } {
  // Lightweight LCS-based line diff for a small summary chip.
  const a = oldText ? oldText.split("\n") : [];
  const b = newText ? newText.split("\n") : [];
  const n = a.length;
  const m = b.length;
  // Bail out on huge inputs — fall back to naive count to keep this O(n+m).
  if (n * m > 250_000) {
    const setA = new Map<string, number>();
    for (const line of a) setA.set(line, (setA.get(line) || 0) + 1);
    let common = 0;
    for (const line of b) {
      const c = setA.get(line) || 0;
      if (c > 0) {
        common++;
        setA.set(line, c - 1);
      }
    }
    return { added: m - common, removed: n - common };
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array(m + 1).fill(0));
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      dp[i][j] = a[i - 1] === b[j - 1] ? dp[i - 1][j - 1] + 1 : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }
  const common = dp[n][m];
  return { added: m - common, removed: n - common };
}

export default function DiffPreview({ path, oldText, newText, maxHeight = 300 }: Props) {
  const { added, removed } = useMemo(
    () => lineDiffSummary(oldText || "", newText || ""),
    [oldText, newText]
  );

  return (
    <div className="diff-preview">
      <div className="diff-preview__header">
        <span className="diff-preview__path" title={path}>
          {path}
        </span>
        <span className="diff-preview__summary" aria-label={`${added} lines added, ${removed} lines removed`}>
          <span className="diff-preview__added">+{added}</span>
          <span className="diff-preview__removed">−{removed}</span>
          <span className="diff-preview__summary-suffix">lines</span>
        </span>
      </div>
      <div className="diff-preview__editor" style={{ height: maxHeight }}>
        <DiffEditor
          original={oldText}
          modified={newText}
          height={maxHeight}
          theme="vs-dark"
          options={{
            readOnly: true,
            renderSideBySide: false,
            renderOverviewRuler: false,
            scrollBeyondLastLine: false,
            minimap: { enabled: false },
            wordWrap: "on",
            fontSize: 12,
            lineNumbers: "on",
            folding: false,
            renderLineHighlight: "none",
            scrollbar: { vertical: "auto", horizontal: "auto" },
          }}
        />
      </div>
    </div>
  );
}
