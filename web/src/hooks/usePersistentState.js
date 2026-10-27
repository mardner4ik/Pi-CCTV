import { useCallback, useState } from 'react'

export function usePersistentState(key, initial) {
  const [value, setValue] = useState(() => {
    try {
      return localStorage.getItem(key) ?? initial
    } catch {
      return initial
    }
  })

  const update = useCallback(
    (next) => {
      setValue(next)
      try {
        localStorage.setItem(key, next)
      } catch {
        return
      }
    },
    [key],
  )

  return [value, update]
}
