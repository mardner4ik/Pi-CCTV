import { useEffect } from 'react'
import Icon from './Icon.jsx'
import { useI18n } from '../i18n/I18nProvider.jsx'

export default function Modal({ title, onClose, children }) {
  const { t } = useI18n()

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div className="modal" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-label={title}>
        <div className="dialog__head">
          <h3>{title}</h3>
          <button type="button" className="btn btn--icon btn--plain" onClick={onClose} aria-label={t('form.close')}>
            <Icon name="close" />
          </button>
        </div>
        {children}
      </div>
    </div>
  )
}
