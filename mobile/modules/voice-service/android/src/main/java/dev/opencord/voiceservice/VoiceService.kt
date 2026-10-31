package dev.opencord.voiceservice

import android.Manifest
import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Runs while this device is in a voice channel, so Android keeps the
 * process, the microphone and playback going with the app in the
 * background. The call itself lives in the Rust core; this only holds the
 * process in the foreground under an ongoing notification that opens the
 * app, and holds audio focus so other apps pause for the call.
 */
class VoiceService : Service() {
  private var focus: AudioFocusRequest? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val title = intent?.getStringExtra(EXTRA_TITLE) ?: DEFAULT_TITLE
    val text = intent?.getStringExtra(EXTRA_TEXT).orEmpty()
    ensureChannel()
    ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(title, text), serviceTypes())
    holdAudioFocus()
    // A restarted service would hold a call the core no longer has.
    return START_NOT_STICKY
  }

  // Swiping the app away ends the call with it, as leaving would.
  override fun onTaskRemoved(rootIntent: Intent?) {
    stopSelf()
  }

  override fun onDestroy() {
    focus?.let { getSystemService(AudioManager::class.java).abandonAudioFocusRequest(it) }
    focus = null
    super.onDestroy()
  }

  /**
   * The core plays and records through cpal's media streams, which take no
   * focus of their own, so other apps would play on over the call. As
   * LiveKit's Android client does for a call, focus is gained for the
   * call's length; in media mode, since the core's streams are media
   * streams (the volume keys move them).
   */
  private fun holdAudioFocus() {
    if (focus != null) return
    val attributes = AudioAttributes.Builder()
      .setUsage(AudioAttributes.USAGE_MEDIA)
      .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
      .build()
    val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
      .setAudioAttributes(attributes)
      // The call goes on whoever takes focus: a phone call takes the
      // microphone itself, and the user leaves voice from the app.
      .setOnAudioFocusChangeListener { }
      .build()
    getSystemService(AudioManager::class.java).requestAudioFocus(request)
    focus = request
  }

  /**
   * Android 14 refuses the microphone type without RECORD_AUDIO granted,
   * so it is claimed only then; playback alone still keeps listening going.
   */
  private fun serviceTypes(): Int {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return 0
    val playback = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
    val microphone =
      ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    return if (microphone && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      playback or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    } else {
      playback
    }
  }

  private fun ensureChannel() {
    val channel = NotificationChannelCompat.Builder(CHANNEL_ID, NotificationManagerCompat.IMPORTANCE_LOW)
      .setName(CHANNEL_NAME)
      .setShowBadge(false)
      .build()
    NotificationManagerCompat.from(this).createNotificationChannel(channel)
  }

  private fun notification(title: String, text: String): Notification {
    val builder = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(R.drawable.opencord_voice_notification)
      .setContentTitle(title)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setSilent(true)
      .setCategory(NotificationCompat.CATEGORY_CALL)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
    if (text.isNotEmpty()) builder.setContentText(text)
    openApp()?.let { builder.setContentIntent(it) }
    return builder.build()
  }

  /** Brings the app's task back as it was, without starting it over. */
  private fun openApp(): PendingIntent? {
    val launch = packageManager.getLaunchIntentForPackage(packageName) ?: return null
    launch.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED
    return PendingIntent.getActivity(
      this,
      0,
      launch,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
  }

  companion object {
    const val EXTRA_TITLE = "title"
    const val EXTRA_TEXT = "text"
    private const val DEFAULT_TITLE = "In voice"
    private const val CHANNEL_ID = "voice"
    private const val CHANNEL_NAME = "Voice"
    private const val NOTIFICATION_ID = 7710
  }
}
