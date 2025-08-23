"use client";

import React, { useEffect, useRef, useImperativeHandle } from 'react'
import PerfectScrollbar from 'perfect-scrollbar'
import 'perfect-scrollbar/css/perfect-scrollbar.css'

export type ScrollAreaHandle = {
  update: () => void
}

type Props = {
  children: React.ReactNode
  height?: string | number
}

const ScrollArea = React.forwardRef<ScrollAreaHandle, Props>(function ScrollArea(
  { children, height = '100vh' },
  ref,
) {
  const hostRef = useRef<HTMLDivElement | null>(null)
  const psRef = useRef<PerfectScrollbar | null>(null)

  useEffect(() => {
    if (hostRef.current) {
      psRef.current = new PerfectScrollbar(hostRef.current, { suppressScrollX: false })
    }
    return () => {
      psRef.current?.destroy()
      psRef.current = null
    }
  }, [])

  useImperativeHandle(ref, () => ({
    update: () => psRef.current?.update(),
  }))

  return (
    <div ref={hostRef} style={{ height, overflow: 'hidden', position: 'relative' }}>
      {children}
    </div>
  )
})

export default ScrollArea


