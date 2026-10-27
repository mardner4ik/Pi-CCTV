import { useState } from 'react'
import CamerasTab from '../components/admin/CamerasTab.jsx'
import UsbTab from '../components/admin/UsbTab.jsx'
import SystemTab from '../components/admin/SystemTab.jsx'
import Icon from '../components/Icon.jsx'
import { useI18n } from '../i18n/I18nProvider.jsx'

const TABS = [
  { id: 'cams', label: 'admin.tab.cams', view: CamerasTab },
  { id: 'usb', label: 'admin.tab.usb', view: UsbTab },
  { id: 'sys', label: 'admin.tab.sys', view: SystemTab },
]

export default function Admin({ enabled }) {
  const { t } = useI18n()
  const [tab, setTab] = useState('cams')

  if (!enabled) {
    return (
      <div className="notice">
        <Icon name="lock" size={22} />
        <div>
          <strong>{t('admin.locked.title')}</strong>
          <p>{t('admin.locked.text')}</p>
        </div>
      </div>
    )
  }

  const Active = TABS.find((x) => x.id === tab).view

  return (
    <div>
      <div className="tabs" role="tablist">
        {TABS.map((x) => (
          <button key={x.id} type="button" role="tab" aria-selected={tab === x.id} onClick={() => setTab(x.id)}>
            {t(x.label)}
          </button>
        ))}
      </div>
      <Active />
    </div>
  )
}
