import { useState } from 'react'
import Modal from '../Modal.jsx'
import Icon from '../Icon.jsx'
import { api } from '../../api/client.js'
import { useI18n } from '../../i18n/I18nProvider.jsx'
import { useToast } from '../Toasts.jsx'

const TYPES = [
  { id: 'a9', icon: 'wifi' },
  { id: 'rtsp', icon: 'camera' },
  { id: 'http', icon: 'globe' },
]

const PLACEHOLDER = {
  rtsp: 'rtsp://user:pass@192.168.1.20:554/stream1',
  http: 'http://192.168.1.21:8080/video',
}

function initial(camera, preset) {
  const src = camera ? camera.source : { type: preset?.type || 'a9' }
  return {
    type: src.type,
    name: camera ? camera.name : preset?.name || '',
    host: src.host || preset?.host || '',
    protocol: src.protocol || 'auto',
    resolution: String(src.resolution || 2),
    url: src.url || '',
  }
}

function toSource(f) {
  if (f.type === 'a9') return { type: 'a9', host: f.host.trim(), protocol: f.protocol, resolution: Number(f.resolution) }
  return { type: f.type, url: f.url.trim() }
}

export default function CameraForm({ camera, preset, onClose, onSaved }) {
  const { t, errorText } = useI18n()
  const toast = useToast()
  const [f, setF] = useState(() => initial(camera, preset))
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [test, setTest] = useState(null)

  const set = (key) => (e) => setF((s) => ({ ...s, [key]: e.target.value }))

  const changeType = (type) => {
    setF((s) => ({ ...s, type }))
    setTest(null)
    setError('')
  }

  const runTest = async () => {
    setError('')
    setTest({ loading: true })
    const res = await api.post('/api/admin/test', { source: toSource(f) })
    if (!res.ok) {
      setTest(null)
      setError(errorText(res.data))
      return
    }
    setTest(res.data.ok ? { image: res.data.image } : { failure: res.data.error })
  }

  const submit = async (e) => {
    e.preventDefault()
    setBusy(true)
    setError('')
    const body = { name: f.name, source: toSource(f) }
    const res = camera ? await api.put(`/api/admin/cameras/${camera.id}`, body) : await api.post('/api/admin/cameras', body)
    setBusy(false)
    if (!res.ok) {
      setError(errorText(res.data))
      return
    }
    toast(t('toast.saved'))
    onSaved()
  }

  return (
    <Modal title={t(camera ? 'form.edit' : 'form.add')} onClose={onClose}>
      <form className="form" onSubmit={submit} noValidate>
        <div className="field">
          <span className="field__label">{t('form.type')}</span>
          <div className="segmented segmented--full" role="group">
            {TYPES.map((ty) => (
              <button key={ty.id} type="button" aria-pressed={f.type === ty.id} onClick={() => changeType(ty.id)}>
                <Icon name={ty.icon} size={16} />
                {t(`type.${ty.id}`)}
              </button>
            ))}
          </div>
        </div>

        <label className="field">
          <span className="field__label">{t('form.name')}</span>
          <input type="text" maxLength={60} value={f.name} onChange={set('name')} autoFocus />
        </label>

        {f.type === 'a9' ? (
          <>
            <label className="field">
              <span className="field__label">{t('form.ip')}</span>
              <input type="text" inputMode="decimal" spellCheck="false" placeholder="192.168.4.153" value={f.host} onChange={set('host')} />
            </label>
            <div className="form__row">
              <label className="field">
                <span className="field__label">{t('form.protocol')}</span>
                <select value={f.protocol} onChange={set('protocol')}>
                  <option value="auto">{t('form.proto.auto')}</option>
                  <option value="pppp">iLnk / PPPP (UDP 32108)</option>
                  <option value="legacy">UDP 8080</option>
                </select>
              </label>
              <label className="field">
                <span className="field__label">{t('form.resolution')}</span>
                <select value={f.resolution} onChange={set('resolution')}>
                  <option value="1">320×240</option>
                  <option value="2">640×480</option>
                  <option value="3">3</option>
                  <option value="4">4</option>
                </select>
              </label>
            </div>
            <p className="help">{t('form.a9.help')}</p>
          </>
        ) : (
          <>
            <label className="field">
              <span className="field__label">{t('form.url')}</span>
              <input type="text" spellCheck="false" autoComplete="off" placeholder={PLACEHOLDER[f.type]} value={f.url} onChange={set('url')} />
            </label>
            <p className="help">{t(`form.${f.type}.help`)}</p>
          </>
        )}

        {test && (
          <div className="preview">
            {test.loading && <p className="muted">{t('form.testing')}</p>}
            {test.image && <img src={test.image} alt="" />}
            {test.failure && <p className="alert alert--bad">{test.failure}</p>}
          </div>
        )}
        {error && <p className="alert alert--bad" role="alert">{error}</p>}

        <div className="form__actions">
          <button type="button" className="btn" onClick={runTest} disabled={test?.loading}>
            <Icon name="play" />
            {t('form.test')}
          </button>
          <span className="spacer" />
          <button type="button" className="btn btn--plain" onClick={onClose}>
            {t('form.cancel')}
          </button>
          <button type="submit" className="btn btn--primary" disabled={busy}>
            <Icon name="check" />
            {t('form.save')}
          </button>
        </div>
      </form>
    </Modal>
  )
}
