"use client";

import dynamic from "next/dynamic";
import React from "react";

// 仅在客户端按需加载 Monaco 编辑器
const Editor = dynamic(
  () => import("@monaco-editor/react").then((m) => m.default),
  {
    ssr: false,
  },
);

type JsonLine = { no: number; text: string };
export type JsonChunk = { range: [number, number]; lines: JsonLine[] };
export type SearchJsonResult = {
  path: string;
  keywords: string[];
  chunks: JsonChunk[];
};

type Props = {
  result: SearchJsonResult;
  keywords: string[];
};

// 构建编辑器显示内容，并维护显示行号（从 1 开始）到原始行号的映射
// 分隔符行使用 null 作为占位
function buildContent(chunks: JsonChunk[]) {
  const lines: string[] = [];
  const map: (number | null)[] = [];
  chunks.forEach((ch, idx) => {
    ch.lines.forEach((l) => {
      lines.push(l.text);
      map.push(l.no);
    });
    if (idx + 1 < chunks.length) {
      lines.push("…");
      map.push(null);
    }
  });
  return { text: lines.join("\n"), map };
}

function computeHighlights(
  text: string,
  map: (number | null)[],
  keywords: string[],
) {
  const nonEmpty = keywords.map((k) => k.trim()).filter(Boolean);
  if (nonEmpty.length === 0) return [] as any[];

  const decorations: any[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const content = lines[i];
    // 分隔符行不做高亮
    if (map[i] == null) continue;
    for (const kw of nonEmpty) {
      // 在一行中查找关键字的所有出现位置
      let start = 0;
      while (true) {
        const pos = content.indexOf(kw, start);
        if (pos === -1) break;
        const fromCol = pos + 1; // Monaco is 1-based
        const toCol = pos + kw.length + 1;
        decorations.push({
          range: {
            startLineNumber: i + 1,
            startColumn: fromCol,
            endLineNumber: i + 1,
            endColumn: toCol,
          },
          options: {
            inlineClassName: "monaco-mark",
          },
        });
        start = pos + Math.max(1, kw.length);
      }
    }
  }
  return decorations;
}

export default function MonacoSnippet({ result, keywords }: Props) {
  const { text, map } = React.useMemo(
    () => buildContent(result.chunks),
    [result],
  );
  const editorRef = React.useRef<any>(null);

  const lineNumbers = React.useCallback(
    (n: number) => {
      const mapped = map[n - 1];
      if (mapped == null) return "…";
      return String(mapped).padStart(6, " ");
    },
    [map],
  );

  const handleMount = React.useCallback(
    (editor: any, monaco: any) => {
      editorRef.current = editor;
      editor.updateOptions({
        readOnly: true,
        wordWrap: "off",
        lineNumbersMinChars: 7,
      });
      const decos = computeHighlights(text, map, keywords);
      if (decos.length) {
        // 应用装饰（高亮）
        editor.createDecorationsCollection(decos);
      }
      // 自定义主题中的标记颜色
      monaco.editor.defineTheme("opsbox-dark", {
        base: "vs-dark",
        inherit: true,
        rules: [],
        colors: {
          "editorLineNumber.foreground": "#71717a",
        },
      });
      editor.updateOptions({ theme: "opsbox-dark" });
    },
    [keywords, map, text],
  );

// 基于行数计算一个合适的高度
  const lineCount = React.useMemo(() => text.split("\n").length, [text]);
  const height = Math.min(Math.max(lineCount * 20, 160), 600);

  return (
    <div
      className="overflow-hidden rounded-md border border-zinc-700/60"
      style={{ height }}
    >
      <Editor
        value={text}
        language="plaintext"
        options={{
          readOnly: true,
          lineNumbers: lineNumbers as any,
          minimap: { enabled: false },
          automaticLayout: true,
          scrollBeyondLastLine: false,
          renderWhitespace: "none",
        }}
        onMount={handleMount}
        theme="opsbox-dark"
      />
    </div>
  );
}
