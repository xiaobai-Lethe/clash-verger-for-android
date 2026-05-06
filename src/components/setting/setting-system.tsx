import { Box } from '@mui/material'

import { ProxyTunCard } from '@/components/home/proxy-tun-card'

import { SettingList } from './mods/setting-comp'

interface Props {
  onError?: (err: Error) => void
}

const SettingSystem = ({ onError: _ }: Props) => {
  return (
    <SettingList title="Android">
      <Box sx={{ px: 2, py: 1.5 }}>
        <ProxyTunCard />
      </Box>
    </SettingList>
  )
}

export default SettingSystem
