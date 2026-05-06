package com.cla.verger.android

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var requestedVpnPermission = false

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    requestVpnPermission()
  }

  override fun onResume() {
    super.onResume()
    if (!requestedVpnPermission) {
      requestVpnPermission()
    }
  }

  override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
    super.onActivityResult(requestCode, resultCode, data)
    requestedVpnPermission = false
    if (requestCode == REQUEST_VPN && resultCode == Activity.RESULT_OK) {
      ClashVpnService.start(this)
    }
  }

  private fun requestVpnPermission() {
    if (requestedVpnPermission) return
    val intent = VpnService.prepare(this)
    if (intent != null) {
      requestedVpnPermission = true
      @Suppress("DEPRECATION")
      startActivityForResult(intent, REQUEST_VPN)
    } else {
      ClashVpnService.start(this)
    }
  }

  companion object {
    private const val REQUEST_VPN = 100
  }
}
