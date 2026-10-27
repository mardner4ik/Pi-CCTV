import { useState } from 'react'
import Logo from '../components/Logo.jsx'
import Icon from '../components/Icon.jsx'
import { api } from '../api/client.js'
import { useI18n } from '../i18n/I18nProvider.jsx'

export default function Login({ onDone }) {
  const { t, lang, toggleLang } = useI18n()
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const submit = async (e) => {
    e.preventDefault()
    setBusy(true)
    setError('')
    const res = await api.post('/api/login', { password })
    setBusy(false)
    if (res.ok) {
      onDone()
      return
    }
    const key = `login.${res.data && res.data.error}`
    setError(res.status !== 0 && t(key) !== key ? t(key) : t('login.failed'))
  }

  return (
    <div className="login">
      <form className="login__card" onSubmit={submit}>
        <Logo size={34} />
        <p className="muted">{t('login.sub')}</p>
        <label className="field">
          <span className="field__label">{t('login.password')}</span>
          <input
            type="password"
            name="password"
            autoComplete="current-password"
            autoFocus
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {error && <p className="alert alert--bad" role="alert">{error}</p>}
        <button type="submit" className="btn btn--primary" disabled={busy}>
          <Icon name="lock" />
          {t('login.submit')}
        </button>
        <button type="button" className="link" onClick={toggleLang}>
          {lang === 'uk' ? 'English' : 'Українська'}
        </button>
      </form>
    </div>
  )
}
