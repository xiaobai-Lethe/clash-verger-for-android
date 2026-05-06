import { getCurrentWindow } from '@tauri-apps/api/window'
import React, { useCallback, useMemo, useState } from 'react'

import { WindowContext } from './window-context'

export const WindowProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const currentWindow = useMemo(() => getCurrentWindow(), [])
  const [decorated, setDecorated] = useState<boolean | null>(null)
  const [maximized, setMaximized] = useState<boolean | null>(null)
  const isAndroid = OS_PLATFORM === 'android'

  const close = useCallback(async () => {
    if (isAndroid) return
    // Delay one frame so the UI can clear :hover before the window hides.
    await new Promise((resolve) => setTimeout(resolve, 20))
    await currentWindow.close()
  }, [currentWindow, isAndroid])
  const minimize = useCallback(async () => {
    if (isAndroid) return
    // Delay one frame so the UI can clear :hover before the window hides.
    await new Promise((resolve) => setTimeout(resolve, 10))
    await currentWindow.minimize()
  }, [currentWindow, isAndroid])

  const toggleMaximize = useCallback(async () => {
    if (isAndroid) return
    if (await currentWindow.isMaximized()) {
      await currentWindow.unmaximize()
      setMaximized(false)
    } else {
      await currentWindow.maximize()
      setMaximized(true)
    }
  }, [currentWindow, isAndroid])

  const toggleFullscreen = useCallback(async () => {
    if (isAndroid) return
    await currentWindow.setFullscreen(!(await currentWindow.isFullscreen()))
  }, [currentWindow, isAndroid])

  const refreshDecorated = useCallback(async () => {
    if (isAndroid) {
      setDecorated(true)
      return true
    }
    const val = await currentWindow.isDecorated()
    setDecorated(val)
    return val
  }, [currentWindow, isAndroid])

  const toggleDecorations = useCallback(async () => {
    if (isAndroid) return
    const currentVal = await currentWindow.isDecorated()
    await currentWindow.setDecorations(!currentVal)
    setDecorated(!currentVal)
  }, [currentWindow, isAndroid])

  const contextValue = useMemo(
    () => ({
      decorated,
      maximized,
      toggleDecorations,
      refreshDecorated,
      minimize,
      close,
      toggleMaximize,
      toggleFullscreen,
      currentWindow,
    }),
    [
      decorated,
      maximized,
      toggleDecorations,
      refreshDecorated,
      minimize,
      close,
      toggleMaximize,
      toggleFullscreen,
      currentWindow,
    ],
  )

  return <WindowContext value={contextValue}>{children}</WindowContext>
}
