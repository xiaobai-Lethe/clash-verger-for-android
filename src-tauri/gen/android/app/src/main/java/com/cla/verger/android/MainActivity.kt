package com.cla.verger.android

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import android.util.Log
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.annotation.Keep
import java.lang.ref.WeakReference

@Keep
class MainActivity : TauriActivity() {
  private var requestedVpnPermission = false
  private var startVpnAfterPermission = false
  private lateinit var vpnPermissionLauncher: ActivityResultLauncher<Intent>
  private lateinit var yamlOpenLauncher: ActivityResultLauncher<Intent>
  private lateinit var yamlSaveLauncher: ActivityResultLauncher<Intent>

  private external fun initRustAndroidBridge()
  private external fun onYamlFileOpened(data: String)
  private external fun getYamlExportData(): String

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    registerVpnPermissionLauncher()
    registerYamlFileLaunchers()
    super.onCreate(savedInstanceState)
    currentActivity = WeakReference(this)
    initRustAndroidBridge()
  }

  override fun onResume() {
    super.onResume()
    currentActivity = WeakReference(this)
  }

  override fun onDestroy() {
    if (currentActivity?.get() === this) {
      currentActivity = null
    }
    super.onDestroy()
  }

  private fun registerVpnPermissionLauncher() {
    vpnPermissionLauncher = registerForActivityResult(
      ActivityResultContracts.StartActivityForResult()
    ) { result ->
      requestedVpnPermission = false
      if (result.resultCode == Activity.RESULT_OK && startVpnAfterPermission) {
        ClashVpnService.start(this)
      }
      startVpnAfterPermission = false
    }
  }

  private fun registerYamlFileLaunchers() {
    yamlOpenLauncher = registerForActivityResult(
      ActivityResultContracts.StartActivityForResult()
    ) { result ->
      if (result.resultCode != Activity.RESULT_OK) return@registerForActivityResult
      val uri = result.data?.data ?: return@registerForActivityResult
      try {
        contentResolver.openInputStream(uri)?.use { input ->
          onYamlFileOpened(input.bufferedReader().readText())
        }
      } catch (error: Throwable) {
        Log.e(TAG, "failed to read selected YAML file", error)
      }
    }

    yamlSaveLauncher = registerForActivityResult(
      ActivityResultContracts.StartActivityForResult()
    ) { result ->
      if (result.resultCode != Activity.RESULT_OK) return@registerForActivityResult
      val uri = result.data?.data ?: return@registerForActivityResult
      try {
        val data = getYamlExportData()
        contentResolver.openOutputStream(uri)?.use { output ->
          output.write(data.toByteArray(Charsets.UTF_8))
        }
      } catch (error: Throwable) {
        Log.e(TAG, "failed to write selected YAML file", error)
      }
    }
  }

  private fun openYamlFilePicker() {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = "*/*"
      putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("text/yaml", "application/yaml", "text/plain"))
    }
    yamlOpenLauncher.launch(intent)
  }

  private fun saveYamlFilePicker(fileName: String) {
    val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = "text/yaml"
      putExtra(Intent.EXTRA_TITLE, fileName)
    }
    yamlSaveLauncher.launch(intent)
  }

  private fun requestVpnPermissionAndStart() {
    if (requestedVpnPermission) {
      Log.i(TAG, "VPN permission request is already in progress")
      return
    }
    val intent = VpnService.prepare(this)
    if (intent != null) {
      Log.i(TAG, "Requesting Android VPN permission from manual proxy toggle")
      requestedVpnPermission = true
      startVpnAfterPermission = true
      vpnPermissionLauncher.launch(intent)
    } else {
      Log.i(TAG, "Android VPN already authorized; starting service from manual proxy toggle")
      startVpnAfterPermission = false
      ClashVpnService.start(this)
    }
  }

  companion object {
    private const val TAG = "MainActivity"
    private var currentActivity: WeakReference<MainActivity>? = null

    @JvmStatic
    @Keep
    fun openYamlFileFromRust(): Boolean {
      val activity = currentActivity?.get()
      if (activity == null) {
        Log.w(TAG, "openYamlFileFromRust ignored because MainActivity is not ready")
        return false
      }
      activity.runOnUiThread {
        activity.openYamlFilePicker()
      }
      return true
    }

    @JvmStatic
    @Keep
    fun saveYamlFileFromRust(fileName: String): Boolean {
      val activity = currentActivity?.get()
      if (activity == null) {
        Log.w(TAG, "saveYamlFileFromRust ignored because MainActivity is not ready")
        return false
      }
      activity.runOnUiThread {
        activity.saveYamlFilePicker(fileName)
      }
      return true
    }

    @JvmStatic
    @Keep
    fun requestVpnFromRust(): Boolean {
      val activity = currentActivity?.get()
      if (activity == null) {
        Log.w(TAG, "requestVpnFromRust ignored because MainActivity is not ready")
        return false
      }
      activity.runOnUiThread {
        activity.requestVpnPermissionAndStart()
      }
      return true
    }

    @JvmStatic
    @Keep
    fun stopVpnFromRust(): Boolean {
      val activity = currentActivity?.get()
      if (activity == null) {
        Log.w(TAG, "stopVpnFromRust ignored because MainActivity is not ready")
        return false
      }
      activity.runOnUiThread {
        ClashVpnService.stop(activity)
      }
      return true
    }
  }
}
