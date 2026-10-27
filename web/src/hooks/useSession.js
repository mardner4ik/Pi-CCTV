import { useCallback, useEffect, useRef, useState } from 'react'
import { api, onUnauthorized } from '../api/client.js'

export function useSession() {
  const [state, setState] = useState({ phase: 'loading', session: null })
  const retry = useRef(0)

  const boot = useCallback(async () => {
    clearTimeout(retry.current)
    const res = await api.get('/api/session')
    if (!res.ok || !res.data) {
      setState((s) => ({ ...s, phase: 'offline' }))
      retry.current = setTimeout(boot, 3000)
      return
    }
    const phase = res.data.auth_required && !res.data.authed ? 'login' : 'ready'
    setState({ phase, session: res.data })
  }, [])

  const logout = useCallback(async () => {
    await api.post('/api/logout')
    setState((s) => ({ ...s, phase: 'login' }))
  }, [])

  useEffect(() => {
    onUnauthorized(() => setState((s) => (s.phase === 'ready' ? { ...s, phase: 'login' } : s)))
    boot()
    return () => clearTimeout(retry.current)
  }, [boot])

  return { ...state, boot, logout }
}
