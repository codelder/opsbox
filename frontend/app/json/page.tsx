"use client";

import React from 'react'
import Link from 'next/link'
import ScrollArea, { type ScrollAreaHandle } from '@/components/ScrollArea'
import MonacoSnippet from '@/components/MonacoSnippet'

// Types aligned with backend/logsearch/src/renderer.rs
// User prefers English comments.
type JsonLine = {
  no: number
  text: string
}

type JsonChunk = {
  range: [number, number]
  lines: JsonLine[]
}

type SearchJsonResult = {
  path: string
  keywords: string[]
  chunks: JsonChunk[]
}

function escapeHtml(input: string): string {
  return input
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

function highlightWithMark(input: string, keywords: string[]): string {
  const nonEmpty = keywords.map(k => k.trim()).filter(Boolean)
  if (nonEmpty.length === 0) return escapeHtml(input)

  let out = ''
  let start = 0
  while (start < input.length) {
    let bestPos: number | null = null
    let bestKw = ''
    for (const kw of nonEmpty) {
      const posRel = input.indexOf(kw, start)
      if (posRel !== -1) {
        const pos = posRel
        if (bestPos == null || pos < bestPos || (pos === bestPos && kw.length > bestKw.length)) {
          bestPos = pos
          bestKw = kw
        }
      }
    }
    if (bestPos == null) {
      out += escapeHtml(input.slice(start))
      break
    } else {
      out += escapeHtml(input.slice(start, bestPos))
      const end = bestPos + bestKw.length
      out += '<mark>' + escapeHtml(input.slice(bestPos, end)) + '</mark>'
      start = end
    }
  }
  return out
}

export default function JsonSearchPage() {
  const [q, setQ] = React.useState('a b')
  const [context, setContext] = React.useState(3)
  const [results, setResults] = React.useState<SearchJsonResult[]>([])
  const [loading, setLoading] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  const areaRef = React.useRef<ScrollAreaHandle>(null)

  async function runSearch(e?: React.FormEvent) {
    e?.preventDefault()
    setLoading(true)
    setResults([])
    setError(null)

    try {
      const apiUrl = `http://127.0.0.1:4000/api/v1/logsearch/stream.ndjson`
      const body = {
        keywords: q.split(/\s+/).filter(Boolean),
        context,
      }
      const res = await fetch(apiUrl, {
        method: 'POST',
        cache: 'no-store',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      })
      if (!res.ok || !res.body) {
        setError(`Request failed: ${res.status}`)
        setLoading(false)
        return
      }

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buf = ''
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        buf += decoder.decode(value, { stream: true })
        // Split by newline for NDJSON. Keep the last partial line in buf.
        const lines = buf.split('\n')
        buf = lines.pop() ?? ''
        for (const line of lines) {
          const trimmed = line.trim()
          if (!trimmed) continue
          try {
            const obj: SearchJsonResult = JSON.parse(trimmed)
            setResults(prev => {
              const next = [...prev, obj]
              // Schedule scroll update after DOM updates.
              requestAnimationFrame(() => areaRef.current?.update())
              return next
            })
          } catch (err) {
            // Ignore malformed JSON line to keep the stream robust
          }
        }
      }
      // Flush any remaining last line (if it's valid JSON without trailing newline)
      const last = buf.trim()
      if (last) {
        try {
          const obj: SearchJsonResult = JSON.parse(last)
          setResults(prev => [...prev, obj])
        } catch (err) {
          // ignore
        }
      }
    } catch (err: any) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  return (
    <main className="container mx-auto max-w-7xl p-6 space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Opsbox JSON Search</h1>
        <nav className="text-sm flex items-center gap-3">
          <Link className="text-sky-400 hover:underline" href="/">Markdown page</Link>
          <span className="text-zinc-400">/</span>
          <span className="text-zinc-200">JSON page</span>
        </nav>
      </header>

      <form onSubmit={runSearch} className="flex items-center gap-3">
        <input
          className="flex-1 rounded-md bg-zinc-900 border border-zinc-800 px-3 py-2 outline-none focus:ring-2 focus:ring-sky-500"
          placeholder="Enter keywords (space-separated)"
          value={q}
          onChange={e => setQ(e.target.value)}
        />
        <input
          type="number"
          className="w-28 rounded-md bg-zinc-900 border border-zinc-800 px-3 py-2 outline-none focus:ring-2 focus:ring-sky-500"
          value={context}
          min={0}
          onChange={e => setContext(Number(e.target.value || 0))}
        />
        <button
          type="submit"
          disabled={loading}
          className="rounded-md bg-sky-600 hover:bg-sky-500 disabled:opacity-50 px-4 py-2"
        >
          {loading ? 'Searching…' : 'Search'}
        </button>
        <button
          type="button"
          onClick={() => { setResults([]); setError(null) }}
          className="rounded-md bg-zinc-700 hover:bg-zinc-600 px-3 py-2"
        >
          Clear
        </button>
      </form>

      {error && (
        <div className="rounded-md border border-red-500 bg-red-950/50 text-red-300 px-3 py-2">
          {error}
        </div>
      )}

      <ScrollArea ref={areaRef}>
        <div className="space-y-6">
          {results.map((r, idx) => (
            <section key={idx} className="rounded-lg border border-zinc-700/60 p-4 bg-zinc-900">
              <h2 className="font-mono text-sm text-zinc-300 break-all">
                s3://test/codeler.tar.gz::{r.path}
              </h2>
              <div className="mt-2 text-xs text-zinc-400">
                Keywords: {r.keywords.join(', ')}
              </div>
              <div className="mt-3">
                <MonacoSnippet result={r as any} keywords={r.keywords} />
              </div>
            </section>
          ))}
          {results.length === 0 && !loading && !error && (
            <div className="text-zinc-400">No results yet. Submit a search to start streaming.</div>
          )}
        </div>
      </ScrollArea>
    </main>
  )
}
