import { useEffect, useRef, useState } from 'react'
import Icon from '../Icon.jsx'
import { api } from '../../api/client.js'
import { useI18n } from '../../i18n/I18nProvider.jsx'
import { useToast } from '../Toasts.jsx'

export default function DiscoverPanel({ onAdd, onClose }) {
  const { t, errorText } = useI18n()
  const toast = useToast()
  const [found, setFound] = useState(null)
  const started = useRef(false)
  const mounted = useRef(false)

  useEffect(() => {
    mounted.current = true
    if (!started.current) {
      started.current = true
      api.post('/api/admin/discover').then((res) => {
        if (!mounted.current) return
        if (res.ok) setFound(res.data)
        else {
          toast(errorText(res.data), true)
          onClose()
        }
      })
    }
    return () => {
      mounted.current = false
    }
  }, [toast, errorText, onClose])

  return (
    <div className="panel">
      <div className="panel__head">
        <strong>{t('admin.found.title')}</strong>
        <button type="button" className="btn btn--icon btn--plain" onClick={onClose} aria-label={t('admin.found.close')}>
          <Icon name="close" />
        </button>
      </div>
      {found === null && <p className="muted">{t('admin.discovering')}</p>}
      {found && found.length === 0 && <p className="muted">{t('admin.found.none')}</p>}
      {found &&
        found.map((f) => (
          <div className="row row--flat" key={f.ip}>
            <div className="row__main">
              <div className="row__title">
                <strong>{f.ip}</strong>
              </div>
              <div className="row__sub">{f.id}</div>
            </div>
            {f.added ? (
              <span className="tag tag--ok">{t('admin.found.added')}</span>
            ) : (
              <button type="button" className="btn" onClick={() => onAdd({ type: 'a9', host: f.ip, name: `A9 ${f.ip.split('.').pop()}` })}>
                <Icon name="plus" />
                {t('admin.found.add')}
              </button>
            )}
          </div>
        ))}
    </div>
  )
}
