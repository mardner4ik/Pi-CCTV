import { useCallback, useState } from 'react'
import CameraRow from './CameraRow.jsx'
import CameraForm from './CameraForm.jsx'
import DiscoverPanel from './DiscoverPanel.jsx'
import Icon from '../Icon.jsx'
import { api } from '../../api/client.js'
import { useI18n } from '../../i18n/I18nProvider.jsx'
import { useToast } from '../Toasts.jsx'
import { usePolling } from '../../hooks/usePolling.js'

export default function CamerasTab() {
  const { t, errorText } = useI18n()
  const toast = useToast()
  const [list, setList] = useState(null)
  const [form, setForm] = useState(null)
  const [discover, setDiscover] = useState(false)

  const load = useCallback(async () => {
    const res = await api.get('/api/admin/cameras')
    if (res.ok && Array.isArray(res.data)) setList(res.data)
  }, [])

  usePolling(load, 3000)

  const closeDiscover = useCallback(() => setDiscover(false), [])

  const onAction = async (cam, action) => {
    if (action === 'edit') {
      setForm({ camera: cam })
      return
    }
    let res
    if (action === 'restart') {
      res = await api.post(`/api/admin/cameras/${cam.id}/restart`)
      if (res.ok) toast(t('toast.reconnect'))
    } else if (action === 'toggle') {
      res = await api.put(`/api/admin/cameras/${cam.id}`, { enabled: !cam.enabled })
    } else if (action === 'delete') {
      if (!window.confirm(t('item.confirm_delete', { name: cam.name }))) return
      res = await api.del(`/api/admin/cameras/${cam.id}`)
      if (res.ok) toast(t('toast.deleted'))
    }
    if (res && !res.ok) toast(errorText(res.data), true)
    load()
  }

  return (
    <section>
      <div className="section-head">
        <div>
          <h2>{t('admin.cams.title')}</h2>
          <p className="muted">{t('admin.cams.hint')}</p>
        </div>
        <div className="section-head__actions">
          <button type="button" className="btn" onClick={() => setDiscover(true)} disabled={discover}>
            <Icon name="search" />
            {t('admin.discover')}
          </button>
          <button type="button" className="btn btn--primary" onClick={() => setForm({})}>
            <Icon name="plus" />
            {t('admin.add')}
          </button>
        </div>
      </div>

      {discover && <DiscoverPanel onAdd={(preset) => setForm({ preset })} onClose={closeDiscover} />}

      {list && list.length === 0 && <p className="empty-note">{t('admin.cams.empty')}</p>}
      <div className="rows">
        {list && list.map((cam) => <CameraRow key={cam.id} cam={cam} onAction={onAction} />)}
      </div>

      {form && (
        <CameraForm
          camera={form.camera}
          preset={form.preset}
          onClose={() => setForm(null)}
          onSaved={() => {
            setForm(null)
            load()
          }}
        />
      )}
    </section>
  )
}
