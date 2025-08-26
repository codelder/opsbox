import type { Metadata } from "next";
import React from "react";
import "../styles/globals.css";

export const metadata: Metadata = {
  title: "Opsbox Search",
  description: "Stream markdown results from backend and render with highlight",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body className="min-h-screen bg-zinc-200 text-zinc-100 antialiased dark:bg-zinc-800 dark:text-zinc-100">
        {children}
      </body>
    </html>
  );
}
