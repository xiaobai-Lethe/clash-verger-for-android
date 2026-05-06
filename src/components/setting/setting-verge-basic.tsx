import { MenuItem, Select } from '@mui/material'
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { DialogRef } from '@/components/base'
import { useVerge } from '@/hooks/use-verge'
import { navItems } from '@/pages/_routers'
import { supportedLanguages } from '@/services/i18n'

import { ConfigViewer } from './mods/config-viewer'
import { GuardState } from './mods/guard-state'
import { MiscViewer } from './mods/misc-viewer'
import { SettingItem, SettingList } from './mods/setting-comp'
import { ThemeModeSwitch } from './mods/theme-mode-switch'
import { ThemeViewer } from './mods/theme-viewer'

interface Props {
  onError?: (err: Error) => void
}

const languageOptions = supportedLanguages.map((code) => {
  const labels: { [key: string]: string } = {
    en: 'English',
    ru: 'Русский',
    zh: '中文',
    fa: 'فارسی',
    tt: 'Татар',
    id: 'Bahasa Indonesia',
    ar: 'العربية',
    ko: '한국어',
    tr: 'Türkçe',
    de: 'Deutsch',
    es: 'Español',
    jp: '日本語',
    zhtw: '繁體中文',
  }
  const label = labels[code] || code
  return { code, label }
})

const SettingVergeBasic = ({ onError }: Props) => {
  const { t } = useTranslation()

  const { verge, patchVerge, mutateVerge } = useVerge()
  const { theme_mode, language, start_page } = verge ?? {}
  const configRef = useRef<DialogRef>(null)
  const miscRef = useRef<DialogRef>(null)
  const themeRef = useRef<DialogRef>(null)

  const onChangeData = (patch: any) => {
    mutateVerge({ ...verge, ...patch }, false)
  }

  return (
    <SettingList title={t('settings.components.verge.basic.title')}>
      <ThemeViewer ref={themeRef} />
      <ConfigViewer ref={configRef} />
      <MiscViewer ref={miscRef} />

      <SettingItem label={t('settings.components.verge.basic.fields.language')}>
        <GuardState
          value={language ?? 'en'}
          onCatch={onError}
          onFormat={(e: any) => e.target.value}
          onChange={(e) => onChangeData({ language: e })}
          onGuard={(e) => patchVerge({ language: e })}
        >
          <Select size="small" sx={{ width: 110, '> div': { py: '7.5px' } }}>
            {languageOptions.map(({ code, label }) => (
              <MenuItem key={code} value={code}>
                {label}
              </MenuItem>
            ))}
          </Select>
        </GuardState>
      </SettingItem>

      <SettingItem
        label={t('settings.components.verge.basic.fields.themeMode')}
      >
        <GuardState
          value={theme_mode}
          onCatch={onError}
          onChange={(e) => onChangeData({ theme_mode: e })}
          onGuard={(e) => patchVerge({ theme_mode: e })}
        >
          <ThemeModeSwitch />
        </GuardState>
      </SettingItem>

      <SettingItem
        label={t('settings.components.verge.basic.fields.startPage')}
      >
        <GuardState
          value={start_page ?? '/'}
          onCatch={onError}
          onFormat={(e: any) => e.target.value}
          onChange={(e) => onChangeData({ start_page: e })}
          onGuard={(e) => patchVerge({ start_page: e })}
        >
          <Select size="small" sx={{ width: 140, '> div': { py: '7.5px' } }}>
            {navItems.map((page: { label: string; path: string }) => {
              return (
                <MenuItem key={page.path} value={page.path}>
                  {t(page.label)}
                </MenuItem>
              )
            })}
          </Select>
        </GuardState>
      </SettingItem>

      <SettingItem
        onClick={() => themeRef.current?.open()}
        label={t('settings.components.verge.basic.fields.themeSetting')}
      />

      <SettingItem
        onClick={() => miscRef.current?.open()}
        label={t('settings.components.verge.basic.fields.misc')}
      />
    </SettingList>
  )
}

export default SettingVergeBasic
