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
  const [pendingRunning, setPendingRunning] = useState(false)
  const { networkInterfaces } = useNetworkInterfaces()

  const { data, refetch } = useQuery({
    queryKey: ['getVpnStatus'],
    queryFn: getVpnStatus,
    refetchInterval: 3000,
    refetchOnWindowFocus: false,
  })

  const running = data?.running ?? false
  const displayRunning = busy ? pendingRunning : running
  const mixedPort = data?.mixed_port ?? 7897
  const coreRunning = data?.core_running ?? false
  const serviceRunning = data?.vpn_service_running ?? false
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
    if (displayRunning) {
      return `Android VPN 已连接，mixed-port ${mixedPort}`
    }
    if (serviceRunning) {
      return coreRunning
        ? 'VPN 服务已启动，正在等待 TUN 建立。'
        : 'VPN 服务已启动，正在等待 mihomo mixed-port。'
    }
    if (coreRunning) {
      return 'mihomo 已运行，但 Android VPN 未连接。'
    }
    return '开启后启动 mihomo 并连接 Android VPN。'
  }, [coreRunning, displayRunning, mixedPort, serviceRunning])

  const toggleCore = useLockFn(async (enabled: boolean) => {
    setBusy(true)
    setPendingRunning(enabled)

    try {
      await (enabled ? startVpn() : stopVpn())
      const status = await waitForVpnState(enabled, refetch)
      if (enabled && !status) {
        showNotice.error('系统代理启动超时，请查看日志')
      }
    } catch (err) {
      showNotice.error(err)
    } finally {
      setBusy(false)
      void refetch()
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
          bgcolor: displayRunning
            ? alpha(theme.palette.success.main, 0.07)
            : alpha(theme.palette.warning.main, 0.06),
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', minWidth: 0 }}>
          {displayRunning ? (
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
                color={displayRunning ? 'success' : 'default'}
                label={busy ? '处理中' : displayRunning ? '运行中' : '已停止'}
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
          disabled={busy}
          checked={displayRunning}
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
        disabled={!displayRunning}
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

const waitForVpnState = async (
  expected: boolean,
  refetch: () => Promise<{ data?: Awaited<ReturnType<typeof getVpnStatus>> }>,
) => {
  const deadline = Date.now() + (expected ? 10_000 : 4_000)

  while (Date.now() < deadline) {
    const result = await refetch()
    if ((result.data?.running ?? false) === expected) {
      return true
    }
    await sleep(500)
  }

  return false
}

const sleep = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms))
