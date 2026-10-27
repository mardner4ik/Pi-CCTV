import { useEffect, useRef } from 'react'

export function usePolling(task, interval, enabled = true) {
  const taskRef = useRef(task)
  taskRef.current = task

  useEffect(() => {
    if (!enabled) return undefined
    let stopped = false
    let timer = 0
    const run = async () => {
      await taskRef.current()
      if (!stopped) timer = setTimeout(run, interval)
    }
    run()
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [interval, enabled])
}
