import { useCallback, useState } from 'react'
import { api } from '../../api/client.js'
import { useI18n } from '../../i18n/I18nProvider.jsx'
import { usePolling } from '../../hooks/usePolling.js'

function uptime(seconds) {
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return `${d ? `${d}d ` : ''}${h}h ${m}m`
}

export default function SystemTab() {
  const { t } = useI18n()
  const [info, setInfo] = useState(null)

  const load = useCallback(async () => {
    const res = await api.get('/api/admin/info')
    if (res.ok && res.data) setInfo(res.data)
  }, [])

  usePolling(load, 5000)

  const rows = info
    ? [
        ['sys.version', `PiCCTV ${info.version}`],
        ['sys.ffmpeg', info.ffmpeg ? info.ffmpeg.replace(/^ffmpeg version\s+/i, '').split(' ')[0] : t('sys.missing')],
        ['sys.mode', info.mode],
        ['sys.resolution', info.resolution],
        ['sys.fps', String(info.fps)],
        ['sys.quality', String(info.quality)],
        ['sys.idle', `${info.idle_secs} ${t('sys.secs')}`],
        ['sys.viewers', String(info.max_viewers)],
        ['sys.netwidth', info.net_width ? `${info.net_width}px` : t('sys.original')],
        ['sys.session', `${info.session_hours} ${t('sys.hours')}`],
        ['sys.uptime', uptime(info.uptime_s)],
        ['sys.cameras', String(info.cameras)],
      ]
    : []

  return (
    <section>
      <div className="section-head">
        <h2>{t('admin.sys.title')}</h2>
      </div>
      <dl className="facts">
        {rows.map(([key, value]) => (
          <div className="facts__item" key={key}>
            <dt>{t(key)}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  )
}
