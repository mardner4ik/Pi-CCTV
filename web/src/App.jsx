import { useState } from 'react'
import Header from './components/Header.jsx'
import Live from './pages/Live.jsx'
import Admin from './pages/Admin.jsx'
import Login from './pages/Login.jsx'
import { useSession } from './hooks/useSession.js'
import { useI18n } from './i18n/I18nProvider.jsx'

export default function App() {
  const { t } = useI18n()
  const { phase, session, boot, logout } = useSession()
  const [view, setView] = useState('live')

  if (phase === 'loading') return null

  if (phase === 'offline' && !session) {
    return (
      <div className="login">
        <p className="muted">{t('offline')}</p>
      </div>
    )
  }

  if (phase === 'login') return <Login onDone={boot} />

  return (
    <div className="shell">
      <Header view={view} onView={setView} canLogout={Boolean(session && session.auth_required)} onLogout={logout} />
      <main className="main">
        {phase === 'offline' && <p className="alert alert--bad">{t('offline')}</p>}
        {view === 'live' ? <Live /> : <Admin enabled={Boolean(session && session.admin)} />}
      </main>
    </div>
  )
}
