import {
  InfoOutlined,
  SettingsOutlined,
  AndroidRounded,
} from '@mui/icons-material'
import { Typography, Stack, Divider, Chip, IconButton } from '@mui/material'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { useVerge } from '@/hooks/use-verge'
import { getCoreStatus, getSystemInfo, getVpnStatus } from '@/services/cmds'
import { version as appVersion } from '@root/package.json'

import { EnhancedCard } from './enhanced-card'

export const SystemInfoCard = () => {
  const { t } = useTranslation()
  const { verge } = useVerge()
  const navigate = useNavigate()

  const [osInfo, setOsInfo] = useState('')
  const [coreInfo, setCoreInfo] = useState<{
    running: boolean
    vpnRunning: boolean
    mixedPort: number
    controllerPort: number
  } | null>(null)

  const statusColor = useMemo(
    () => (coreInfo?.vpnRunning ? 'success' : 'default'),
    [coreInfo?.vpnRunning],
  )

  // 初始化系统信息
  useEffect(() => {
    getSystemInfo()
      .then((info) => {
        const lines = (info || '').split('\n')
        if (lines.length > 0) {
          const sysName = lines[0]?.split(': ')[1] || 'Android'
          let sysVersion = lines[1]?.split(': ')[1] || ''

          if (
            sysName &&
            sysVersion.toLowerCase().startsWith(sysName.toLowerCase())
          ) {
            sysVersion = sysVersion.substring(sysName.length).trim()
          }

          setOsInfo(`${sysName} ${sysVersion}`)
        }
      })
      .catch(console.error)
  }, [])

  useEffect(() => {
    let cancelled = false

    const refreshStatus = async () => {
      try {
        const [core, vpn] = await Promise.all([getCoreStatus(), getVpnStatus()])
        if (cancelled) return
        setCoreInfo({
          running: core.running,
          vpnRunning: vpn.running,
          mixedPort: core.mixed_port,
          controllerPort: core.controller_port,
        })
      } catch (error) {
        console.warn('[SystemInfoCard] failed to get Android runtime status', error)
      }
    }

    void refreshStatus()
    const timer = window.setInterval(refreshStatus, 5000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [])

  // 导航到设置页面
  const goToSettings = useCallback(() => {
    navigate('/settings')
  }, [navigate])

  if (!verge) return null

  return (
    <EnhancedCard
      title={t('home.components.systemInfo.title')}
      icon={<InfoOutlined />}
      iconColor="error"
      action={
        <IconButton
          size="small"
          onClick={goToSettings}
          title={t('home.components.systemInfo.actions.settings')}
        >
          <SettingsOutlined fontSize="small" />
        </IconButton>
      }
    >
      <Stack spacing={1.5}>
        <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
          <Typography variant="body2" color="text.secondary">
            {t('home.components.systemInfo.fields.osInfo')}
          </Typography>
          <Typography variant="body2" sx={{ fontWeight: 'medium' }}>
            {osInfo}
          </Typography>
        </Stack>
        <Divider />
        <Stack
          direction="row"
          sx={{ justifyContent: 'space-between', alignItems: 'center' }}
        >
          <Typography variant="body2" color="text.secondary">
            VPN
          </Typography>
          <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
            <Chip
              size="small"
              label={
                coreInfo?.vpnRunning
                  ? t('shared.statuses.enabled')
                  : t('shared.statuses.disabled')
              }
              color={statusColor}
              variant={coreInfo?.vpnRunning ? 'filled' : 'outlined'}
            />
          </Stack>
        </Stack>
        <Divider />
        <Stack
          direction="row"
          sx={{ justifyContent: 'space-between', alignItems: 'center' }}
        >
          <Typography variant="body2" color="text.secondary">
            mihomo
          </Typography>
          <Typography variant="body2" sx={{ fontWeight: 'medium' }}>
            <AndroidRounded
              sx={{
                color: coreInfo?.running ? 'success.main' : 'text.disabled',
                fontSize: 16,
                mr: 0.5,
                verticalAlign: 'text-bottom',
              }}
            />
            {coreInfo?.running ? '运行中' : '已停止'}
          </Typography>
        </Stack>
        <Divider />
        <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
          <Typography variant="body2" color="text.secondary">
            mixed-port
          </Typography>
          <Typography variant="body2" sx={{ fontWeight: 'medium' }}>
            {coreInfo?.mixedPort ?? 7897}
          </Typography>
        </Stack>
        <Divider />
        <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
          <Typography variant="body2" color="text.secondary">
            controller
          </Typography>
          <Typography variant="body2" sx={{ fontWeight: 'medium' }}>
            127.0.0.1:{coreInfo?.controllerPort ?? 9097}
          </Typography>
        </Stack>
        <Divider />
        <Stack direction="row" sx={{ justifyContent: 'space-between' }}>
          <Typography variant="body2" color="text.secondary">
            {t('home.components.systemInfo.fields.vergeVersion')}
          </Typography>
          <Typography variant="body2" sx={{ fontWeight: 'medium' }}>
            v{appVersion}
          </Typography>
        </Stack>
      </Stack>
    </EnhancedCard>
  )
}
