import { useCallback, useState } from 'react'
import Icon from '../Icon.jsx'
import { api } from '../../api/client.js'
import { useI18n } from '../../i18n/I18nProvider.jsx'
import { useToast } from '../Toasts.jsx'
import { usePolling } from '../../hooks/usePolling.js'

export default function UsbTab() {
  const { t, errorText } = useI18n()
  const toast = useToast()
  const [devices, setDevices] = useState(null)

  const load = useCallback(async () => {
    const res = await api.get('/api/admin/devices')
    if (res.ok && Array.isArray(res.data)) setDevices(res.data)
  }, [])

  usePolling(load, 3000)

  const toggle = async (d) => {
    const res = await api.post(`/api/admin/devices/${d.node}/hidden`, { hidden: !d.hidden })
    if (!res.ok) toast(errorText(res.data), true)
    load()
  }

  return (
    <section>
      <div className="section-head">
        <div>
          <h2>{t('admin.usb.title')}</h2>
          <p className="muted">{t('admin.usb.hint')}</p>
        </div>
      </div>
      {devices && devices.length === 0 && <p className="empty-note">{t('admin.usb.empty')}</p>}
      <div className="rows">
        {devices &&
          devices.map((d) => {
            const usable = d.camera && !d.hidden
            return (
              <div className={`row${usable ? '' : ' row--off'}`} key={d.node}>
                <div className="row__icon">
                  <Icon name={d.camera ? 'usb' : 'chip'} />
                </div>
                <div className="row__main">
                  <div className="row__title">
                    <strong>{d.card || d.node}</strong>
                    {d.camera ? (
                      <span className={`tag tag--${d.hidden ? 'warn' : 'ok'}`}>{t(d.hidden ? 'tag.hidden' : 'tag.camera')}</span>
                    ) : (
                      <span className="tag tag--bad">{t(`reason.${d.reason}`)}</span>
                    )}
                  </div>
                  <div className="row__sub">{[`/dev/${d.node}`, d.driver, d.formats.join(' ')].filter(Boolean).join(' · ')}</div>
                </div>
                {d.camera && (
                  <div className="row__actions">
                    <button type="button" className="btn" onClick={() => toggle(d)}>
                      <Icon name={d.hidden ? 'eye' : 'eyeOff'} />
                      {t(d.hidden ? 'admin.usb.show' : 'admin.usb.hide')}
                    </button>
                  </div>
                )}
              </div>
            )
          })}
      </div>
    </section>
  )
}
