import { useEffect, useState } from 'react'

export function usePageVisible() {
  const [visible, setVisible] = useState(!document.hidden)

  useEffect(() => {
    const onChange = () => setVisible(!document.hidden)
    document.addEventListener('visibilitychange', onChange)
    return () => document.removeEventListener('visibilitychange', onChange)
  }, [])

  return visible
}
