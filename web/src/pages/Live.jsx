import { useEffect, useRef, useState } from 'react'
import CameraCard from '../components/live/CameraCard.jsx'
import Icon from '../components/Icon.jsx'
import { api } from '../api/client.js'
import { useI18n } from '../i18n/I18nProvider.jsx'
import { usePolling } from '../hooks/usePolling.js'
import { usePageVisible } from '../hooks/usePageVisible.js'
import { usePersistentState } from '../hooks/usePersistentState.js'

const LAYOUTS = ['auto', '1', '2', '3']

export default function Live() {
  const { t } = useI18n()
  const visible = usePageVisible()
  const [cams, setCams] = useState(null)
  const [offline, setOffline] = useState(false)
  const [epoch, setEpoch] = useState(0)
  const [focusId, setFocusId] = useState(null)
  const [stored, setLayout] = usePersistentState('layout', 'auto')
  const failures = useRef(0)
  const layout = LAYOUTS.includes(stored) ? stored : 'auto'

  usePolling(
    async () => {
      const res = await api.get('/api/cameras', { timeout: 4000 })
      if (res.ok && Array.isArray(res.data)) {
        if (failures.current >= 2) setEpoch((e) => e + 1)
        failures.current = 0
        setOffline(false)
        setCams(res.data)
      } else if (res.status !== 401) {
        failures.current += 1
        if (failures.current >= 2) setOffline(true)
      }
    },
    2000,
    visible,
  )

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === 'Escape') setFocusId(null)
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [])

  const list = cams || []
  const liveCount = list.filter((c) => c.state === 'live').length

  return (
    <section>
      <div className="toolbar">
        <span className="muted">
          <strong>{liveCount}</strong>/{list.length} {t('online')}
        </span>
        <div className="segmented" role="group">
          {LAYOUTS.map((l) => (
            <button key={l} type="button" aria-pressed={layout === l} onClick={() => setLayout(l)}>
              {l === 'auto' ? t('layout.auto') : l}
            </button>
          ))}
        </div>
      </div>

      {offline && <p className="alert alert--bad">{t('offline')}</p>}

      {cams && list.length === 0 ? (
        <div className="empty">
          <Icon name="camera" size={32} />
          <strong>{t('live.empty.title')}</strong>
          <p className="muted">{t('live.empty.text')}</p>
        </div>
      ) : (
        <div className="grid" data-layout={layout} style={{ '--cols': layout === 'auto' ? undefined : layout }}>
          {list.map((cam) => (
            <CameraCard
              key={cam.id}
              cam={cam}
              epoch={epoch}
              lite={list.length >= 4 && focusId !== cam.id}
              focused={focusId === cam.id}
              onToggleFocus={() => setFocusId((id) => (id === cam.id ? null : cam.id))}
            />
          ))}
        </div>
      )}
    </section>
  )
}
