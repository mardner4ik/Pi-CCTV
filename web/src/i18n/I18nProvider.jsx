import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import uk from './uk.js'
import en from './en.js'

const dictionaries = { uk, en }
const I18nContext = createContext(null)

function initialLang() {
  try {
    const saved = localStorage.getItem('lang')
    if (saved && dictionaries[saved]) return saved
  } catch {
    return 'en'
  }
  return (navigator.language || '').toLowerCase().startsWith('uk') ? 'uk' : 'en'
}

export function I18nProvider({ children }) {
  const [lang, setLang] = useState(initialLang)

  useEffect(() => {
    document.documentElement.lang = lang
    try {
      localStorage.setItem('lang', lang)
    } catch {
      return
    }
  }, [lang])

  const t = useCallback(
    (key, vars) => {
      let text = dictionaries[lang][key] ?? en[key] ?? key
      if (vars) for (const [k, v] of Object.entries(vars)) text = text.replace(`{${k}}`, v)
      return text
    },
    [lang],
  )

  const errorText = useCallback(
    (data) => {
      const key = `err.${data && data.error}`
      return dictionaries[lang][key] ?? en[key] ?? t('err.generic')
    },
    [lang, t],
  )

  const value = useMemo(
    () => ({ lang, t, errorText, toggleLang: () => setLang((l) => (l === 'uk' ? 'en' : 'uk')) }),
    [lang, t, errorText],
  )

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n() {
  return useContext(I18nContext)
}
