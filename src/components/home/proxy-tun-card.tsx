import {
  PauseCircleOutlineRounded,
  PlayCircleOutlineRounded,
  RestartAltRounded,
} from '@mui/icons-material'
import { LoadingButton } from '@mui/lab'
import { Box, Chip, Stack, Typography, alpha, useTheme } from '@mui/material'
import { useQuery } from '@tanstack/react-query'
import { useLockFn } from 'ahooks'
import { useMemo, useState } from 'react'

import { Switch } from '@/components/base'
import { useNetworkInterfaces } from '@/hooks/use-network'
import {
  getVpnStatus,
  restartCore,
  startVpn,
  stopVpn,
} from '@/services/cmds'
import { showNotice } from '@/services/notice-service'

export const ProxyTunCard = () => {
  const theme = useTheme()
  const [busy, setBusy] = useState(false)
  const { networkInterfaces } = useNetworkInterfaces()

  const { data, refetch, isFetching } = useQuery({
    queryKey: ['getVpnStatus'],
    queryFn: getVpnStatus,
    refetchInterval: 3000,
    refetchOnWindowFocus: false,
  })

  const running = data?.running ?? false
  const mixedPort = data?.mixed_port ?? 7897
  const lanEndpoint = useMemo(() => {
    const candidates = networkInterfaces
      .flatMap((item) =>
        (item.addr ?? [])
          .map((addr) => addr.V4?.ip)
          .filter((ip): ip is string => isPrivateLanIp(ip))
          .map((ip) => ({ ip, name: item.name, score: lanInterfaceScore(item.name) })),
      )
      .filter((item) => item.score >= 0)
      .sort((a, b) => b.score - a.score)

    return candidates[0] ?? null
  }, [networkInterfaces])
  const localAddress = `127.0.0.1:${mixedPort}`
  const lanAddress = lanEndpoint
    ? `${lanEndpoint.ip}:${mixedPort} (${lanEndpoint.name})`
    : '未检测到局域网 IP'

  const description = useMemo(() => {
    if (running) {
      return `mihomo 正在运行，mixed-port ${mixedPort}`
    }
    return '开启后启动 mihomo，本机应用可通过系统代理端口连接。'
  }, [mixedPort, running])

  const toggleCore = useLockFn(async (enabled: boolean) => {
    try {
      setBusy(true)
      if (enabled) {
        await startVpn()
      } else {
        await stopVpn()
      }
      await refetch()
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  const onRestart = useLockFn(async () => {
    try {
      setBusy(true)
      await restartCore()
      await refetch()
      showNotice.success('核心已重启')
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
    }
  })

  return (
    <Box sx={{ width: '100%' }}>
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          p: 1,
          pr: 1.5,
          borderRadius: 1.5,
          bgcolor: running
            ? alpha(theme.palette.success.main, 0.07)
            : alpha(theme.palette.warning.main, 0.06),
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', minWidth: 0 }}>
          {running ? (
            <PlayCircleOutlineRounded sx={{ color: 'success.main', mr: 1 }} />
          ) : (
            <PauseCircleOutlineRounded sx={{ color: 'text.disabled', mr: 1 }} />
          )}
          <Box sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={1} alignItems="center">
              <Typography sx={{ fontWeight: 600, fontSize: 15 }}>
                系统代理
              </Typography>
              <Chip
                size="small"
                color={running ? 'success' : 'default'}
                label={running ? '运行中' : '已停止'}
              />
            </Stack>
            <Typography
              variant="caption"
              sx={{
                display: 'block',
                mt: 0.5,
                color: 'text.secondary',
                wordBreak: 'break-word',
              }}
            >
              {description}
            </Typography>
          </Box>
        </Box>

        <Switch
          edge="end"
          disabled={busy || isFetching}
          checked={running}
          onChange={(_, checked) => toggleCore(checked)}
        />
      </Box>

      <Stack spacing={0.75} sx={{ mt: 1 }}>
        <AddressRow label="本机连接" value={localAddress} />
        <AddressRow label="局域网连接" value={lanAddress} />
      </Stack>

      <LoadingButton
        fullWidth
        size="small"
        sx={{ mt: 1 }}
        loading={busy}
        disabled={!running}
        startIcon={<RestartAltRounded />}
        onClick={onRestart}
      >
        重启 mihomo
      </LoadingButton>
    </Box>
  )
}

const AddressRow = ({ label, value }: { label: string; value: string }) => (
  <Box
    sx={{
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: 1,
      px: 1.25,
      py: 0.75,
      borderRadius: 1,
      bgcolor: 'action.hover',
    }}
  >
    <Typography variant="caption" sx={{ color: 'text.secondary' }}>
      {label}
    </Typography>
    <Typography
      variant="caption"
      sx={{
        fontFamily: 'monospace',
        color: 'text.primary',
        wordBreak: 'break-all',
        textAlign: 'right',
      }}
    >
      {value}
    </Typography>
  </Box>
)

const isPrivateLanIp = (ip?: string) => {
  if (!ip) return false
  const parts = ip.split('.').map((item) => Number(item))
  if (parts.length !== 4 || parts.some((item) => Number.isNaN(item))) {
    return false
  }
  const [a, b] = parts
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)
}

const lanInterfaceScore = (name: string) => {
  const normalized = name.toLowerCase()
  if (
    normalized.startsWith('tun') ||
    normalized.startsWith('rmnet') ||
    normalized.startsWith('ccmni') ||
    normalized.startsWith('clat') ||
    normalized.startsWith('dummy') ||
    normalized.startsWith('lo') ||
    normalized.startsWith('p2p')
  ) {
    return -1
  }
  if (normalized.startsWith('wlan')) return 100
  if (normalized.startsWith('ap') || normalized.startsWith('swlan')) return 90
  if (normalized.startsWith('eth')) return 80
  if (normalized.startsWith('usb') || normalized.startsWith('rndis')) return 70
  return 0
}
