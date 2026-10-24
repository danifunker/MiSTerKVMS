package com.danifunker.misterkvms.model

import android.app.Application
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.danifunker.misterkvms.AndroidCore
import com.danifunker.misterkvms.core.ConnectionState
import com.danifunker.misterkvms.core.Controller
import com.danifunker.misterkvms.core.ControllerEvent
import com.danifunker.misterkvms.core.ControllerObserver
import com.danifunker.misterkvms.core.ControllerState
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.core.NativeSurface
import com.danifunker.misterkvms.core.StreamLimits
import com.danifunker.misterkvms.core.demoHostProfile

/**
 * UI-facing state for the app: the saved hosts and the one open session.
 * The session logic lives in the core's Controller (as on the iPhone); this
 * mirrors its state for Compose and adds what's Android-specific: the video
 * surface, audio focus and the app lifecycle. Used on the main thread only.
 */
class AppModel(private val app: Application) {
    private val main = Handler(Looper.getMainLooper())
    private val prefs = app.getSharedPreferences("settings", Context.MODE_PRIVATE)

    val core: Controller = Controller(object : ControllerObserver {
        // Core events come on background threads.
        override fun onEvent(event: ControllerEvent) {
            main.post { handle(event) }
        }
    })

    var state: ControllerState by mutableStateOf(core.state())
        private set
    var profiles: List<HostProfile> by mutableStateOf(core.profiles())
        private set

    /** The first-run screen: try the demo, or go on to the MiSTers. */
    var showWelcome by mutableStateOf(!prefs.getBoolean(Pref.WELCOMED, false))
        private set

    /** Control given up from here: no input is sent (it would take control
     *  back) until the user takes control again. */
    var watching by mutableStateOf(false)
        private set

    /** The host that was open when the app went to the background. */
    private var suspendedId: String? = null

    private var demoModeState by mutableStateOf(prefs.getBoolean(Pref.DEMO_MODE, false))

    init {
        // A touch screen captures as soon as a host connects, whatever the
        // desktop apps' Capture automatically setting says.
        core.forceAutoCapture()
        if (showWelcome && profiles.any { !it.isDemo }) {
            prefs.edit().putBoolean(Pref.WELCOMED, true).apply()
            showWelcome = false
        }
        syncDemoHost()
        // Other viewers see this name ("Pixel 9 has control"). The core
        // falls back to "MiSTerKVMS" where it can't read a computer name.
        if (core.viewerName() == "MiSTerKVMS") core.setViewerName(defaultDeviceName())
    }

    private fun handle(event: ControllerEvent) {
        when (event) {
            ControllerEvent.CHANGED -> state = core.state()
            ControllerEvent.PROFILES_CHANGED -> {
                profiles = core.profiles()
                state = core.state()
            }
            // A touch screen is always "capturing" while connected.
            ControllerEvent.CAPTURE_REQUESTED ->
                if (!someoneElseInControl && !watching) core.setCapturing(true)
            ControllerEvent.RELEASE_REQUESTED -> core.setCapturing(false)
            ControllerEvent.PREVIEWS_CHANGED -> {}
        }
    }

    // MARK: State

    val profile: HostProfile? get() = state.profileId?.let { id -> profiles.firstOrNull { it.id == id } }

    val isConnected: Boolean get() = state.connection is ConnectionState.Connected

    /** A capture server stream that someone else controls. */
    val someoneElseInControl: Boolean
        get() = state.stream?.let { !it.haveControl && it.holder.isNotEmpty() } ?: false

    /** This phone's input reaches the MiSTer. */
    val inControl: Boolean get() = state.stream != null && !watching && !someoneElseInControl

    var muted: Boolean
        get() = state.muted
        set(value) = core.setMuted(value)

    // MARK: Hosts

    fun save(profile: HostProfile) = core.saveProfile(profile)

    fun delete(id: String) {
        core.deleteProfile(id)
        profiles = core.profiles()
    }

    // MARK: Demo (docs/APP-STORE.md S1)

    /** The Demo host is in the list: the test pattern, with the input it
     *  receives listed on screen instead of sent to a MiSTer. */
    var demoMode: Boolean
        get() = demoModeState
        set(value) {
            demoModeState = value
            prefs.edit().putBoolean(Pref.DEMO_MODE, value).apply()
            if (!value && profile?.isDemo == true) close()
            syncDemoHost()
        }

    val isDemo: Boolean get() = profile?.isDemo == true

    /** The first-run screen's answer: `demo` turns demo mode on and opens it. */
    fun welcomed(demo: Boolean) {
        prefs.edit().putBoolean(Pref.WELCOMED, true).apply()
        showWelcome = false
        if (demo) {
            demoMode = true
            profiles.firstOrNull { it.isDemo }?.let { open(it) }
        }
    }

    /** The Demo host saved while demo mode is on, gone while it's off. */
    private fun syncDemoHost() {
        val have = profiles.any { it.isDemo }
        if (demoMode && !have) {
            core.saveProfile(demoHostProfile())
        } else if (!demoMode && have) {
            core.deleteProfile(demoHostProfile().id)
        }
        profiles = core.profiles()
    }

    // MARK: Session

    fun open(profile: HostProfile) {
        if (profile.id == state.profileId) return
        watching = false
        requestAudioFocus()
        core.open(profile.id)
        state = core.state()
    }

    fun close() {
        core.releaseAll()
        core.closeHost()
        state = core.state()
        abandonAudioFocus()
    }

    /** The next (+1) or previous (-1) host, wrapping around. */
    fun step(offset: Int) {
        val id = state.profileId ?: return
        val i = profiles.indexOfFirst { it.id == id }
        if (profiles.size < 2 || i < 0) return
        switchTo(profiles[(i + offset + profiles.size) % profiles.size])
    }

    /** Leave the open MiSTer (keys and buttons released) for another one. */
    fun switchTo(profile: HostProfile) {
        if (profile.id == state.profileId) return
        core.releaseAll()
        core.closeHost()
        open(profile)
    }

    /** Off screen: let go of the MiSTer cleanly, reconnect when back. */
    fun suspend() {
        val id = state.profileId ?: return
        suspendedId = id
        close()
    }

    fun resume() {
        val id = suspendedId ?: return
        suspendedId = null
        profiles.firstOrNull { it.id == id }?.let { open(it) }
    }

    fun takeControl() {
        watching = false
        core.takeControl()
        core.setCapturing(true)
    }

    /** Let another viewer have the MiSTer; this phone only watches. */
    fun releaseControl() {
        watching = true
        core.releaseAll()
        core.releaseControl()
        core.setCapturing(false)
    }

    // MARK: Video surface

    /** The SurfaceView's window (`AndroidCore.windowFromSurface`). */
    private var window = 0L

    /** The surface exists, or changed size: draw into it. */
    fun surfaceChanged(surface: android.view.Surface, widthPx: Int, heightPx: Int, density: Float) {
        if (window == 0L) {
            window = AndroidCore.windowFromSurface(surface)
            if (window == 0L) return
            core.attachSurface(
                NativeSurface.Android(window.toULong()), widthPx.toUInt(), heightPx.toUInt(), density)
        } else {
            core.resizeSurface(widthPx.toUInt(), heightPx.toUInt(), density)
        }
        askForScreenSize(widthPx, heightPx)
    }

    /** The surface is going (background, or the session screen closed):
     *  stop drawing before it's released. The connection stays. */
    fun surfaceDestroyed() {
        if (window == 0L) return
        core.detachSurface()
        AndroidCore.releaseWindow(window)
        window = 0L
    }

    /** No bigger picture than the phone shows: the server scales it down. */
    private fun askForScreenSize(widthPx: Int, heightPx: Int) {
        core.setStreamLimits(StreamLimits(widthPx.toUInt(), heightPx.toUInt(), null, null))
    }

    // MARK: Audio focus

    private val audio = app.getSystemService(AudioManager::class.java)
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                .build())
        .build()

    private fun requestAudioFocus() { audio.requestAudioFocus(focus) }
    private fun abandonAudioFocus() { audio.abandonAudioFocusRequest(focus) }

    // MARK: This phone's name

    private fun defaultDeviceName(): String =
        Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME)
            ?.takeIf { it.isNotBlank() } ?: Build.MODEL
}

/** The demo host (core: `demo_host_profile`). */
val HostProfile.isDemo: Boolean get() = source == "demo"

/** The stream's name, from its address (`mkvm://pc/name`). */
val HostProfile.streamName: String
    get() {
        val path = stream.substringAfter("://")
        if (!path.contains('/')) return ""
        return android.net.Uri.decode(path.substringAfter('/').trim('/'))
    }

val HostProfile.subtitle: String
    get() = when {
        isDemo -> "Test pattern; your input is listed on screen"
        streamName.isNotEmpty() -> "Stream $streamName"
        misterHost.isNotEmpty() -> misterHost
        else -> "No stream chosen"
    }
