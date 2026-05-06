package com.cla.verger.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.annotation.Keep
import java.io.File
import java.net.InetSocketAddress
import java.net.Socket
import kotlin.concurrent.thread

@Keep
class ClashVpnService : VpnService() {
  private var tun: ParcelFileDescriptor? = null
  @Volatile private var started = false
  @Volatile private var supervisorRunning = false
  @Volatile private var stopping = false

  external fun TProxyStartService(configPath: String, fd: Int)
  external fun TProxyStopService()
  external fun TProxyGetStats(): LongArray

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    Log.i(TAG, "onStartCommand action=${intent?.action ?: "start"}")
    startForeground(NOTIFICATION_ID, buildNotification())

    if (intent?.action == ACTION_STOP) {
      Log.i(TAG, "Stopping Android VPN service")
      serviceRunning = false
      teardownTunnel()
      stopSelf()
      return START_NOT_STICKY
    }

    serviceRunning = true
    startSupervisor()

    return START_NOT_STICKY
  }

  override fun onRevoke() {
    stopSelf()
    super.onRevoke()
  }

  override fun onDestroy() {
    stopping = true
    serviceRunning = false
    teardownTunnel()
    super.onDestroy()
  }

  private fun startSupervisor() {
    if (supervisorRunning) return
    supervisorRunning = true
    stopping = false
    thread(name = "clash-vpn-supervisor") {
      Log.i(TAG, "VPN supervisor started")
      while (!stopping) {
        if (!started) {
          if (waitForLocalPort("127.0.0.1", SOCKS_PORT, 2_000)) {
            try {
              establishTunnel()
            } catch (error: Throwable) {
              Log.e(TAG, "failed to establish VPN tunnel", error)
              teardownTunnel()
              Thread.sleep(1000)
            }
          } else {
            Thread.sleep(500)
          }
          continue
        }

        if (!isLocalPortOpen("127.0.0.1", SOCKS_PORT)) {
          Log.w(TAG, "mihomo mixed-port closed; releasing VPN tunnel until core restarts")
          teardownTunnel()
          Thread.sleep(500)
          continue
        }

        Thread.sleep(1000)
      }
      supervisorRunning = false
      Log.i(TAG, "VPN supervisor stopped")
    }
  }

  private fun establishTunnel() {
    if (started || tun != null) return

    val builder = Builder()
      .setSession("Clash Verge Rev")
      .setMtu(MTU)
      .addAddress("198.18.0.1", 30)
      .addAddress("fdfe:dcba:9876::1", 126)
      .addRoute("0.0.0.0", 0)
      .addRoute("::", 0)
      .addDnsServer("1.1.1.1")
      .addDnsServer("8.8.8.8")
      .setBlocking(false)

    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
      builder.addDisallowedApplication(packageName)
    }

    val descriptor = builder.establish()
      ?: throw IllegalStateException("VpnService.Builder.establish returned null")

    val configPath = writeHevConfig()
    tun = descriptor
    TProxyStartService(configPath, descriptor.fd)
    started = true
    tunnelRunning = true
    Log.i(TAG, "VPN tunnel started with hev-socks5-tunnel, socks=127.0.0.1:$SOCKS_PORT")
  }

  private fun teardownTunnel() {
    try {
      TProxyStopService()
    } catch (error: Throwable) {
      Log.w(TAG, "failed to stop hev socks tunnel", error)
    }
    try {
      tun?.close()
    } catch (error: Throwable) {
      Log.w(TAG, "failed to close TUN descriptor", error)
    }
    tun = null
    started = false
    tunnelRunning = false
  }

  private fun waitForLocalPort(host: String, port: Int, timeoutMs: Long): Boolean {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (System.currentTimeMillis() < deadline) {
      if (isLocalPortOpen(host, port)) return true
      Thread.sleep(250)
    }
    return false
  }

  private fun isLocalPortOpen(host: String, port: Int): Boolean {
    return try {
      Socket().use { socket ->
        socket.connect(InetSocketAddress(host, port), 500)
        true
      }
    } catch (_: Throwable) {
      false
    }
  }

  private fun writeHevConfig(): String {
    val file = File(filesDir, "hev-socks5-tunnel.yaml")
    file.writeText(
      """
      tunnel:
        name: tun0
        mtu: $MTU
        multi-queue: false
        ipv4: 198.18.0.1
        ipv6: 'fdfe:dcba:9876::1'

      socks5:
        port: $SOCKS_PORT
        address: 127.0.0.1
        udp: udp

      misc:
        task-stack-size: 86016
        connect-timeout: 10000
        tcp-read-write-timeout: 300000
        udp-read-write-timeout: 60000
        log-file: stderr
        log-level: info
      """.trimIndent(),
    )
    return file.absolutePath
  }

  private fun buildNotification(): Notification {
    val channelId = "clash_vpn"
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val channel = NotificationChannel(
        channelId,
        "Clash VPN",
        NotificationManager.IMPORTANCE_LOW,
      )
      getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      Notification.Builder(this, channelId)
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(this)
    }

    return builder
      .setContentTitle("Clash Verge Rev")
      .setContentText("VPN tunnel is active")
      .setSmallIcon(android.R.drawable.stat_sys_download_done)
      .build()
  }

  companion object {
    private const val TAG = "ClashVpnService"
    private const val NOTIFICATION_ID = 1001
    private const val MTU = 8500
    private const val SOCKS_PORT = 7897
    private const val ACTION_STOP = "com.cla.verger.android.STOP_VPN"
    @Volatile private var serviceRunning = false
    @Volatile private var tunnelRunning = false

    init {
      System.loadLibrary("hev-socks5-tunnel")
    }

    fun start(context: Context) {
      Log.i(TAG, "Starting Android VPN service")
      val intent = Intent(context, ClashVpnService::class.java)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        context.startForegroundService(intent)
      } else {
        context.startService(intent)
      }
    }

    fun stop(context: Context) {
      Log.i(TAG, "Requesting Android VPN service stop")
      val intent = Intent(context, ClashVpnService::class.java).setAction(ACTION_STOP)
      serviceRunning = false
      context.startService(intent)
    }

    @JvmStatic
    @Keep
    fun isRunning(): Boolean = serviceRunning

    @JvmStatic
    @Keep
    fun isTunnelRunning(): Boolean = tunnelRunning
  }
}
