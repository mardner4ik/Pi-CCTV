import Icon from '../Icon.jsx'
import { useI18n } from '../../i18n/I18nProvider.jsx'

const SOURCE_ICON = { a9: 'wifi', rtsp: 'camera', http: 'globe' }
const STATE_TAG = {
  live: ['ok', 'state.live'],
  starting: ['warn', 'state.connecting'],
  error: ['bad', 'state.lost'],
}

function maskUrl(url) {
  return url.replace(/\/\/[^/@]*@/, '//***@')
}

function describe(source) {
  if (source.type === 'a9') return `${source.host} · ${source.protocol}`
  return maskUrl(source.url)
}

export default function CameraRow({ cam, onAction }) {
  const { t } = useI18n()
  const tag = cam.enabled ? STATE_TAG[cam.state] : null

  return (
    <div className={`row${cam.enabled ? '' : ' row--off'}`}>
      <div className="row__icon">
        <Icon name={SOURCE_ICON[cam.source.type] || 'camera'} />
      </div>
      <div className="row__main">
        <div className="row__title">
          <strong>{cam.name}</strong>
          <span className="tag">{t(`type.${cam.source.type}`)}</span>
          {!cam.enabled && <span className="tag">{t('tag.disabled')}</span>}
          {tag && <span className={`tag tag--${tag[0]}`}>{t(tag[1])}</span>}
        </div>
        <div className="row__sub">{describe(cam.source)}</div>
        {cam.enabled && cam.state === 'error' && cam.error && <div className="row__error">{cam.error}</div>}
      </div>
      <div className="row__actions">
        <button type="button" className="btn btn--icon" title={t('item.reconnect')} aria-label={t('item.reconnect')} onClick={() => onAction(cam, 'restart')}>
          <Icon name="refresh" />
        </button>
        <button
          type="button"
          className="btn btn--icon"
          title={cam.enabled ? t('item.disable') : t('item.enable')}
          aria-label={cam.enabled ? t('item.disable') : t('item.enable')}
          onClick={() => onAction(cam, 'toggle')}
        >
          <Icon name="power" />
        </button>
        <button type="button" className="btn btn--icon" title={t('item.edit')} aria-label={t('item.edit')} onClick={() => onAction(cam, 'edit')}>
          <Icon name="edit" />
        </button>
        <button type="button" className="btn btn--icon btn--danger" title={t('item.delete')} aria-label={t('item.delete')} onClick={() => onAction(cam, 'delete')}>
          <Icon name="trash" />
        </button>
      </div>
    </div>
  )
}
