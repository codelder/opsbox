"use client";

import { useDarkMode } from "@/hook/dark-mode";
import "@/styles/fontawesome/css/all.min.css";
import "@/styles/globals.css";
import React from "react";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  useDarkMode();
  return (
    <html lang="en" className={`h-full`}>
      <body className={`h-full bg-white dark:bg-black`}>{children}</body>
    </html>
  );
}
