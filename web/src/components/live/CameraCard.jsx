import { useCallback, useEffect, useRef, useState } from 'react'
import Icon from '../Icon.jsx'
import { useI18n } from '../../i18n/I18nProvider.jsx'

const BLANK = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7'
const SOURCE_ICON = { usb: 'usb', a9: 'wifi', rtsp: 'camera', http: 'globe', demo: 'play' }
const STALE_MS = 5000

function resolveState(cam, broken) {
  if (broken) return 'lost'
  if (cam.state === 'live') return (cam.frame_age_ms ?? 0) > STALE_MS ? 'lost' : 'live'
  if (cam.state === 'error') return 'lost'
  return 'connecting'
}

export default function CameraCard({ cam, lite, focused, epoch, onToggleFocus }) {
  const { t } = useI18n()
  const [src, setSrc] = useState(BLANK)
  const [broken, setBroken] = useState(false)
  const retries = useRef(0)
  const timer = useRef(0)
  const video = useRef(null)

  const connect = useCallback(() => {
    clearTimeout(timer.current)
    setSrc(`/stream/${encodeURIComponent(cam.id)}?t=${Date.now()}${lite ? '&fps=10' : ''}`)
  }, [cam.id, lite])

  useEffect(() => {
    retries.current = 0
    setBroken(false)
    connect()
    return () => clearTimeout(timer.current)
  }, [connect, epoch])

  const onLoad = (e) => {
    if (e.currentTarget.src.startsWith('data:')) return
    retries.current = 0
    setBroken(false)
  }

  const onError = (e) => {
    if (e.currentTarget.src.startsWith('data:')) return
    setBroken(true)
    retries.current = Math.min(retries.current + 1, 5)
    clearTimeout(timer.current)
    timer.current = setTimeout(connect, Math.min(1000 * 2 ** retries.current, 15000))
  }

  const fullscreen = () => {
    const el = video.current
    if (!el) return
    if (document.fullscreenElement) document.exitFullscreen()
    else el.requestFullscreen?.()
  }

  const state = resolveState(cam, broken)
  const stats = []
  if (state === 'live') stats.push(`${cam.fps.toFixed(1)} fps`)
  if (cam.mode) stats.push(cam.mode)
  stats.push(`${cam.viewers} ${cam.viewers === 1 ? t('viewers.one') : t('viewers.many')}`)

  return (
    <article className={`cam cam--${state}${focused ? ' cam--focused' : ''}`}>
      <header className="cam__head">
        <Icon name={SOURCE_ICON[cam.source] || 'camera'} size={16} />
        <span className="cam__name">{cam.name}</span>
        <span className="cam__state">
          <span className="dot" />
          {t(`state.${state}`)}
        </span>
      </header>
      <div className="cam__video" ref={video} onDoubleClick={onToggleFocus}>
        <img src={src} alt="" draggable="false" decoding="async" onLoad={onLoad} onError={onError} />
        {state !== 'live' && (
          <div className="cam__message">
            <strong>{state === 'lost' ? t('signal.lost') : t('signal.connecting')}</strong>
            {state === 'lost' && (cam.error || broken) && <span>{cam.error || t('signal.failed')}</span>}
          </div>
        )}
      </div>
      <footer className="cam__foot">
        <span className="cam__stats">{stats.join(' · ')}</span>
        <div className="cam__tools">
          <a
            className="btn btn--icon btn--plain"
            href={`/snapshot/${encodeURIComponent(cam.id)}?download=1`}
            download
            title={t('cam.snapshot')}
            aria-label={t('cam.snapshot')}
          >
            <Icon name="snapshot" />
          </a>
          <button
            type="button"
            className="btn btn--icon btn--plain"
            onClick={onToggleFocus}
            title={focused ? t('cam.unfocus') : t('cam.focus')}
            aria-label={focused ? t('cam.unfocus') : t('cam.focus')}
          >
            <Icon name={focused ? 'shrink' : 'enlarge'} />
          </button>
          <button
            type="button"
            className="btn btn--icon btn--plain"
            onClick={fullscreen}
            title={t('cam.fullscreen')}
            aria-label={t('cam.fullscreen')}
          >
            <Icon name="expand" />
          </button>
        </div>
      </footer>
    </article>
  )
}
