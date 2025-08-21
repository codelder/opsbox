"use client";

import React from 'react'
import ScrollArea, { type ScrollAreaHandle } from '@/components/ScrollArea'
import ReactMarkdown from 'react-markdown'
import rehypeRaw from 'rehype-raw'

export default function Page() {
    const [q, setQ] = React.useState('a b')
    const [context, setContext] = React.useState(3)
    const [output, setOutput] = React.useState('')
    const [loading, setLoading] = React.useState(false)

    const areaRef = React.useRef<ScrollAreaHandle>(null)

    async function runSearch(e?: React.FormEvent) {
        console.log('runSearch')
        e?.preventDefault()
        setLoading(true)
        setOutput('')
        try {
            const apiUrl = `http://127.0.0.1:4000/api/v1/logsearch/stream`;
            const body = {
                q: q.split(/\s+/).filter(Boolean),
                context,
            };
            console.log('fetch POST:', apiUrl, body)
            const res = await fetch(apiUrl, {
                method: 'POST',
                cache: 'no-store',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            })
            if (!res.ok || !res.body) {
                setOutput(`# 请求失败\n\n状态: ${res.status}`)
                setLoading(false)
                return
            }
            const reader = res.body.getReader()
            const decoder = new TextDecoder()
            while (true) {
                console.log('reader.read()')
                const { done, value } = await reader.read()
                console.log('done', done)
                console.log('value', value)
                if (done) break
                setOutput(prev => {
                    const next = prev + decoder.decode(value)
                    // 让滚动条在内容增量后更新尺寸
                    requestAnimationFrame(() => areaRef.current?.update())
                    return next
                })
            }
        } catch (err: any) {
            setOutput(`# 出错\n\n${String(err)}`)
        } finally {
            setLoading(false)
        }
    }

    return (
        <main className="container mx-auto max-w-7xl p-6 space-y-6">
            <h1 className="text-2xl font-semibold">Opsbox 搜索</h1>
            <form onSubmit={runSearch} className="flex items-center gap-3">
                <input
                    className="flex-1 rounded-md bg-zinc-900 border border-zinc-800 px-3 py-2 outline-none focus:ring-2 focus:ring-sky-500"
                    placeholder="输入关键字"
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
                    {loading ? '搜索中…' : '搜索'}
                </button>
            </form>

            <ScrollArea ref={areaRef}>
                <article className="prose prose-sm dark:prose-invert max-w-none">
                    {/* 使用 react-markdown 渲染，启用 rehype-raw 以支持返回中的原生 HTML（如 <pre>/<mark>） */}
                    <ReactMarkdown rehypePlugins={[rehypeRaw]}>{output}</ReactMarkdown>
                </article>
            </ScrollArea>
        </main>
    )
}