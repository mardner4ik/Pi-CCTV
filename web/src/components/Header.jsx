import Logo from './Logo.jsx'
import Icon from './Icon.jsx'
import { useI18n } from '../i18n/I18nProvider.jsx'

export default function Header({ view, onView, canLogout, onLogout }) {
  const { t, lang, toggleLang } = useI18n()

  return (
    <header className="header">
      <Logo />
      <nav className="nav">
        <button type="button" className="nav__link" aria-current={view === 'live' ? 'page' : undefined} onClick={() => onView('live')}>
          {t('nav.live')}
        </button>
        <button type="button" className="nav__link" aria-current={view === 'admin' ? 'page' : undefined} onClick={() => onView('admin')}>
          {t('nav.admin')}
        </button>
      </nav>
      <div className="header__right">
        <button type="button" className="btn btn--plain" onClick={toggleLang}>
          {lang.toUpperCase()}
        </button>
        {canLogout && (
          <button type="button" className="btn btn--icon btn--plain" onClick={onLogout} title={t('nav.logout')} aria-label={t('nav.logout')}>
            <Icon name="logout" />
          </button>
        )}
      </div>
    </header>
  )
}
