"use client";

import React from 'react'
import PerfectScrollbar from 'react-perfect-scrollbar'
import 'perfect-scrollbar/css/perfect-scrollbar.css'

export default function ScrollRoot({ children }: { children: React.ReactNode }) {
  return (
    <PerfectScrollbar options={{ suppressScrollX: true }} style={{ height: '100vh' }}>
      <div className="min-h-screen">{children}</div>
    </PerfectScrollbar>
  )
}


