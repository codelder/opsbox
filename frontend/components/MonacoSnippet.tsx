"use client";

import React from 'react'
import dynamic from 'next/dynamic'

// Lazy-load Monaco editor on client only
const Editor = dynamic(() => import('@monaco-editor/react').then(m => m.default), {
  ssr: false,
})

type JsonLine = { no: number; text: string }
export type JsonChunk = { range: [number, number]; lines: JsonLine[] }
export type SearchJsonResult = { path: string; keywords: string[]; chunks: JsonChunk[] }

type Props = {
  result: SearchJsonResult
  keywords: string[]
}

// Build the editor content and a mapping from displayed line index (1-based)
// to original line number (or null for separators)
function buildContent(chunks: JsonChunk[]) {
  const lines: string[] = []
  const map: (number | null)[] = []
  chunks.forEach((ch, idx) => {
    ch.lines.forEach(l => {
      lines.push(l.text)
      map.push(l.no)
    })
    if (idx + 1 < chunks.length) {
      lines.push('…')
      map.push(null)
    }
  })
  return { text: lines.join('\n'), map }
}

function computeHighlights(text: string, map: (number | null)[], keywords: string[]) {
  const nonEmpty = keywords.map(k => k.trim()).filter(Boolean)
  if (nonEmpty.length === 0) return [] as any[]

  const decorations: any[] = []
  const lines = text.split('\n')
  for (let i = 0; i < lines.length; i++) {
    const content = lines[i]
    // Don't highlight separator lines
    if (map[i] == null) continue
    for (const kw of nonEmpty) {
      // Find all occurrences of kw in content
      let start = 0
      while (true) {
        const pos = content.indexOf(kw, start)
        if (pos === -1) break
        const fromCol = pos + 1 // Monaco is 1-based
        const toCol = pos + kw.length + 1
        decorations.push({
          range: {
            startLineNumber: i + 1,
            startColumn: fromCol,
            endLineNumber: i + 1,
            endColumn: toCol,
          },
          options: {
            inlineClassName: 'monaco-mark',
          },
        })
        start = pos + Math.max(1, kw.length)
      }
    }
  }
  return decorations
}

export default function MonacoSnippet({ result, keywords }: Props) {
  const { text, map } = React.useMemo(() => buildContent(result.chunks), [result])
  const editorRef = React.useRef<any>(null)

  const lineNumbers = React.useCallback(
    (n: number) => {
      const mapped = map[n - 1]
      if (mapped == null) return '…'
      return String(mapped).padStart(6, ' ')
    },
    [map],
  )

  const handleMount = React.useCallback((editor: any, monaco: any) => {
    editorRef.current = editor
    editor.updateOptions({
      readOnly: true,
      wordWrap: 'off',
      lineNumbersMinChars: 7,
    })
    const decos = computeHighlights(text, map, keywords)
    if (decos.length) {
      // Apply decorations
      editor.createDecorationsCollection(decos)
    }
    // Custom theme mark color
    monaco.editor.defineTheme('opsbox-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [],
      colors: {
        'editorLineNumber.foreground': '#71717a',
      },
    })
    editor.updateOptions({ theme: 'opsbox-dark' })
  }, [keywords, map, text])

  // Compute a reasonable height based on the lines count
  const lineCount = React.useMemo(() => text.split('\n').length, [text])
  const height = Math.min(Math.max(lineCount * 20, 160), 600)

  return (
    <div className="rounded-md overflow-hidden border border-zinc-700/60" style={{ height }}>
      <Editor
        value={text}
        language="plaintext"
        options={{
          readOnly: true,
          lineNumbers: lineNumbers as any,
          minimap: { enabled: false },
          automaticLayout: true,
          scrollBeyondLastLine: false,
          renderWhitespace: 'none',
        }}
        onMount={handleMount}
        theme="opsbox-dark"
      />
    </div>
  )
}
