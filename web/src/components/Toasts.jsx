import { createContext, useCallback, useContext, useState } from 'react'

const ToastContext = createContext(() => {})

export function ToastProvider({ children }) {
  const [items, setItems] = useState([])

  const push = useCallback((text, bad = false) => {
    const id = Math.random().toString(36).slice(2)
    setItems((list) => [...list, { id, text, bad }])
    setTimeout(() => setItems((list) => list.filter((i) => i.id !== id)), 3200)
  }, [])

  return (
    <ToastContext.Provider value={push}>
      {children}
      <div className="toasts" aria-live="polite">
        {items.map((i) => (
          <div key={i.id} className={`toast${i.bad ? ' toast--bad' : ''}`}>
            {i.text}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  )
}

export function useToast() {
  return useContext(ToastContext)
}
