import { invoke } from '@tauri-apps/api/core'

export type LogLevel = 'debug' | 'info' | 'warning' | 'error' | 'silent'

export interface Message {
  type: 'Text' | 'Binary'
  data: string
}

export interface Traffic {
  up: number
  down: number
}

export interface ProxyDelay {
  delay: number
}

export interface BaseConfig {
  port?: number
  socksPort?: number
  redirPort?: number
  tproxyPort?: number
  mixedPort?: number
  allowLan?: boolean
  bindAddress?: string
  mode?: string
  logLevel?: LogLevel
  ipv6?: boolean
  externalController?: string
  secret?: string
  tun?: Record<string, unknown>
  [key: string]: unknown
}

export interface ProxyProvider extends Partial<IProxyProviderItem> {
  name?: string
  type?: string
  vehicleType?: string
  proxies: IProxyItem[]
  updatedAt?: string
  subscriptionInfo?: IProxyProviderItem['subscriptionInfo']
  [key: string]: unknown
}

export interface RuleProvider extends Partial<IRuleProviderItem> {
  name?: string
  type?: string
  vehicleType?: string
  behavior?: string
  ruleCount?: number
  updatedAt?: string
  [key: string]: unknown
}

export interface Rule {
  type: string
  payload: string
  proxy: string
  size?: number
  [key: string]: unknown
}

type Listener = (message: Message) => void

const httpInvoke = <T>(path: string, init?: RequestInit) =>
  invoke<T>('mihomo_http', {
    path,
    method: init?.method ?? 'GET',
    body: init?.body ? String(init.body) : null,
  })

const wsPath = (kind: string, query?: string) =>
  query ? `${kind}?${query}` : kind

export class MihomoWebSocket {
  private static sockets = new Set<MihomoWebSocket>()

  private ws: WebSocket | null = null
  private listeners = new Set<Listener>()

  private constructor(private readonly path: string) {}

  static async connect_traffic() {
    return MihomoWebSocket.connect(wsPath('traffic'))
  }

  static async connect_memory() {
    return MihomoWebSocket.connect(wsPath('memory'))
  }

  static async connect_connections() {
    return MihomoWebSocket.connect(wsPath('connections'))
  }

  static async connect_logs(level: LogLevel = 'info') {
    return MihomoWebSocket.connect(wsPath('logs', `level=${level}`))
  }

  static async connect(path: string) {
    const socket = new MihomoWebSocket(path)
    MihomoWebSocket.sockets.add(socket)
    await socket.open()
    return socket
  }

  static cleanupAll() {
    for (const socket of MihomoWebSocket.sockets) {
      void socket.close()
    }
    MihomoWebSocket.sockets.clear()
  }

  addListener(listener: Listener) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  async close() {
    MihomoWebSocket.sockets.delete(this)
    if (this.ws && this.ws.readyState <= WebSocket.OPEN) {
      this.ws.close()
    }
    this.ws = null
  }

  private async open() {
    const controller = await invoke<string>('mihomo_ws_url', {
      path: this.path,
    })

    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(controller)
      this.ws = ws

      ws.onopen = () => resolve()
      ws.onerror = () => {
        this.emitText('Websocket error')
        reject(new Error(`failed to connect mihomo websocket: ${this.path}`))
      }
      ws.onclose = () => this.emitText('Websocket error: closed')
      ws.onmessage = (event) => {
        const data =
          typeof event.data === 'string' ? event.data : String(event.data)
        this.emitText(data)
      }
    })
  }

  private emitText(data: string) {
    const message: Message = { type: 'Text', data }
    this.listeners.forEach((listener) => listener(message))
  }
}

export const getVersion = () =>
  invoke<{ version: string; meta?: boolean }>('mihomo_version')

export const getBaseConfig = () => httpInvoke<BaseConfig>('/configs')

export const getProxies = () =>
  httpInvoke<{ proxies: Record<string, IProxyItem> }>('/proxies')

export const getProxyProviders = () =>
  httpInvoke<{ providers: Record<string, ProxyProvider> }>('/providers/proxies')

export const getRules = () => httpInvoke<{ rules: Rule[] }>('/rules')

export const getRuleProviders = () =>
  httpInvoke<{ providers: Record<string, RuleProvider> }>('/providers/rules')

export const getConnections = () => httpInvoke<IConnections>('/connections')

export const closeConnection = (id: string) =>
  httpInvoke<void>(`/connections/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })

export const closeAllConnections = () =>
  httpInvoke<void>('/connections', { method: 'DELETE' })

export const selectNodeForGroup = (group: string, name: string) =>
  httpInvoke<void>(`/proxies/${encodeURIComponent(group)}`, {
    method: 'PUT',
    body: JSON.stringify({ name }),
  })

export const delayProxyByName = (
  name: string,
  url: string,
  timeout: number,
) =>
  httpInvoke<ProxyDelay>(
    `/proxies/${encodeURIComponent(name)}/delay?timeout=${timeout}&url=${encodeURIComponent(url)}`,
  )

export const delayGroup = (group: string, url: string, timeout: number) =>
  httpInvoke<Record<string, ProxyDelay>>(
    `/group/${encodeURIComponent(group)}/delay?timeout=${timeout}&url=${encodeURIComponent(url)}`,
  )

export const healthcheckProxyProvider = (name: string) =>
  httpInvoke<void>(`/providers/proxies/${encodeURIComponent(name)}/healthcheck`)

export const updateProxyProvider = (name: string) =>
  httpInvoke<void>(`/providers/proxies/${encodeURIComponent(name)}`, {
    method: 'PUT',
  })

export const updateRuleProvider = (name: string) =>
  httpInvoke<void>(`/providers/rules/${encodeURIComponent(name)}`, {
    method: 'PUT',
  })

export const updateGeo = () =>
  httpInvoke<void>('/configs/geo', { method: 'POST' })

export const upgradeCore = () => invoke<void>('upgrade_core')
