"use client";

import React from 'react'
import Link from 'next/link'
import {useVirtualizer} from '@tanstack/react-virtual'

// Types aligned with backend/logsearch/src/renderer.rs
// User prefers English comments.
type JsonLine = { no: number; text: string }
type JsonChunk = { range: [number, number]; lines: JsonLine[] }
type SearchJsonResult = { path: string; keywords: string[]; chunks: JsonChunk[] }

type Row = | { kind: 'header'; key: string; path: string; keywords: string[] } | {
    kind: 'code';
    key: string;
    no: number;
    text: string;
    keywords: string[]
} | { kind: 'sep'; key: string }

function escapeHtml(s: string) {
    return s
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;')
}

function highlightWithMark(input: string, keywords: string[]): string {
    const ks = keywords.map(k => k.trim()).filter(Boolean)
    if (ks.length === 0) return escapeHtml(input)
    let out = ''
    let i = 0
    while (i < input.length) {
        let bestPos = Number.POSITIVE_INFINITY;
        let bestKw = ''
        for (const kw of ks) {
            const p = input.indexOf(kw, i)
            if (p !== -1 && (p < bestPos || (p === bestPos && kw.length > bestKw.length))) {
                bestPos = p;
                bestKw = kw
            }
        }
        if (!isFinite(bestPos)) {
            out += escapeHtml(input.slice(i));
            break
        }
        out += escapeHtml(input.slice(i, bestPos))
        out += `<mark class="bg-yellow-400/80 rounded">${escapeHtml(input.slice(bestPos, bestPos + bestKw.length))}</mark>`
        i = bestPos + bestKw.length
    }
    return out
}

function toRows(r: SearchJsonResult): Row[] {
    const rows: Row[] = []
    rows.push({kind: 'header', key: `h:${r.path}:${crypto.randomUUID()}`, path: r.path, keywords: r.keywords})
    r.chunks.forEach((ch, ci) => {
        ch.lines.forEach(l => {
            rows.push({kind: 'code', key: `c:${r.path}:${l.no}`, no: l.no, text: l.text, keywords: r.keywords})
        })
        if (ci + 1 < r.chunks.length) rows.push({kind: 'sep', key: `s:${r.path}:${ci}`})
    })
    return rows
}

const MAX_ROWS = 30000

export default function Page() {
    const parentRef = React.useRef<HTMLDivElement | null>(null)
    const [rows, setRows] = React.useState<Row[]>([])
    const rowsRef = React.useRef<Row[]>([])
    const queueRef = React.useRef<Row[]>([])
    const rafRef = React.useRef<number | null>(null)
    const abortRef = React.useRef<AbortController | null>(null)

    const scheduleFlush = React.useCallback(() => {
        if (rafRef.current != null) return
        rafRef.current = requestAnimationFrame(() => {
            rafRef.current = null
            if (queueRef.current.length === 0) return
            const next = rowsRef.current.concat(queueRef.current)
            queueRef.current = []
            if (next.length > MAX_ROWS) next.splice(0, next.length - MAX_ROWS)
            rowsRef.current = next
            setRows(next)
        })
    }, [])

    const start = React.useCallback(async (keywords: string[], context: number) => {
        abortRef.current?.abort()
        abortRef.current = new AbortController()
        rowsRef.current = []
        queueRef.current = []
        setRows([])

        const res = await fetch('http://127.0.0.1:4000/api/v1/logsearch/stream.ndjson', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            cache: 'no-store',
            signal: abortRef.current.signal,
            body: JSON.stringify({keywords, context}),
        })
        if (!res.ok || !res.body) return

        const reader = res.body.getReader()
        const decoder = new TextDecoder()
        let buf = ''

        while (true) {
            const {done, value} = await reader.read()
            if (done) break
            buf += decoder.decode(value, {stream: true})
            const lines = buf.split('\n')
            buf = lines.pop() ?? ''
            for (const line of lines) {
                const t = line.trim()
                if (!t) continue
                try {
                    const obj = JSON.parse(t) as SearchJsonResult
                    queueRef.current.push(...toRows(obj))
                } catch {
                }
            }
            scheduleFlush()
        }
        const last = buf.trim()
        if (last) {
            try {
                queueRef.current.push(...toRows(JSON.parse(last) as SearchJsonResult));
                scheduleFlush()
            } catch {
            }
        }
    }, [scheduleFlush])

    const rowVirtualizer = useVirtualizer({
        count: rows.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => 22,
        overscan: 10, // enable dynamic measurement for wrapped lines
        measureElement: (el) => (el as HTMLElement).getBoundingClientRect().height,
    })

    React.useEffect(() => () => {
        abortRef.current?.abort()
        if (rafRef.current) cancelAnimationFrame(rafRef.current)
    }, [])

    return (<main className="container mx-auto max-w-7xl p-6 space-y-6">
            <div className="flex items-center justify-between">
                <h1 className="text-2xl font-semibold">Opsbox Virtual Search</h1>
                <nav className="text-sm flex items-center gap-3">
                    <Link className="text-sky-400 hover:underline" href="/">Markdown page</Link>
                    <span className="text-zinc-400">/</span>
                    <Link className="text-sky-400 hover:underline" href="/json">JSON page</Link>
                    <span className="text-zinc-400">/</span>
                    <span className="text-zinc-200">Virtual page</span>
                </nav>
            </div>

            <form
                onSubmit={(e) => {
                    e.preventDefault()
                    const fd = new FormData(e.currentTarget as HTMLFormElement)
                    const q = String(fd.get('q') || '')
                    const ctx = Number(fd.get('ctx') || 3)
                    start(q.split(/\s+/).filter(Boolean), ctx)
                }}
                className="flex items-center gap-3"
            >
                <input name="q"
                       className="flex-1 rounded-md bg-zinc-900 border border-zinc-800 px-3 py-2 outline-none focus:ring-2 focus:ring-sky-500"
                       placeholder="Enter keywords (space-separated)"/>
                <input name="ctx" type="number" min={0} defaultValue={3}
                       className="w-28 rounded-md bg-zinc-900 border border-zinc-800 px-3 py-2 outline-none focus:ring-2 focus:ring-sky-500"/>
                <button className="rounded-md bg-sky-600 hover:bg-sky-500 px-4 py-2">Search</button>
                <button type="button" onClick={() => abortRef.current?.abort()}
                        className="rounded-md bg-zinc-700 hover:bg-zinc-600 px-3 py-2">Stop
                </button>
                <button type="button" onClick={() => {
                    rowsRef.current = [];
                    queueRef.current = [];
                    setRows([])
                }} className="rounded-md bg-zinc-700 hover:bg-zinc-600 px-3 py-2">Clear
                </button>
            </form>

            <div ref={parentRef} className="h-[70vh] overflow-auto rounded-md border border-zinc-700/60 bg-zinc-900">
                {(() => {
                    const items = rowVirtualizer.getVirtualItems();
                    const paddingTop = items.length ? items[0].start : 0;
                    const paddingBottom = items.length ? rowVirtualizer.getTotalSize() - items[items.length - 1].end : 0;
                    return (<div style={{paddingTop, paddingBottom}}>
                            {items.map(vi => {
                                const row = rows[vi.index];
                                return (<div ref={rowVirtualizer.measureElement} key={row.key} className="">
                                        {row.kind === 'header' && (
                                            <div className="px-3 py-2 border-b border-zinc-800 bg-zinc-900">
                                                <div
                                                    className="font-mono text-sm text-zinc-300 break-all">s3://test/codeler.tar.gz::{row.path}</div>
                                                <div
                                                    className="text-xs text-zinc-400">Keywords: {row.keywords.join(', ')}</div>
                                            </div>)}
                                        {row.kind === 'code' && (<div className="px-3 py-[2px] text-sm">
                                                <div className="grid items-start font-mono min-w-0"
                                                     style={{gridTemplateColumns: '48px 18px 1fr'}}>
                                                    <span
                                                        className="text-zinc-500 select-none text-right tabular-nums">{String(row.no).padStart(6, ' ')}</span>
                                                    <span className="text-zinc-500 select-none text-center"
                                                          aria-hidden>|</span>
                                                    <span className="min-w-0 whitespace-pre-wrap break-all"
                                                          style={{overflowWrap: 'anywhere'}}
                                                          dangerouslySetInnerHTML={{__html: highlightWithMark(row.text, row.keywords)}}/>
                                                </div>
                                            </div>)}
                                        {row.kind === 'sep' && (
                                            <div className="px-3 py-1 text-zinc-500 text-sm">…</div>)}
                                    </div>)
                            })}
                        </div>);
                })()}
            </div>
        </main>)
}
