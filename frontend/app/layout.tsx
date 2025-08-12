import React from 'react'
import './globals.css'
import type { Metadata } from 'next'

export const metadata: Metadata = {
  title: 'Opsbox Search',
  description: 'Stream markdown results from backend and render with highlight',
}

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="zh-CN">
      <body className="min-h-screen bg-zinc-200 dark:bg-zinc-800 text-zinc-100 dark:text-zinc-100 antialiased">
        {children}
      </body>
    </html>
  )
}
