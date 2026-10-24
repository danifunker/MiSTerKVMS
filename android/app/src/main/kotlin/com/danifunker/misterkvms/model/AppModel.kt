package com.danifunker.misterkvms.model

import android.app.Application
import android.content.ClipData
import android.content.ClipboardManager
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.Surface
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.FileProvider
import com.danifunker.misterkvms.AndroidCore
import com.danifunker.misterkvms.core.ConnectionState
import com.danifunker.misterkvms.core.Controller
import com.danifunker.misterkvms.core.ControllerEvent
import com.danifunker.misterkvms.core.ControllerException
import com.danifunker.misterkvms.core.ControllerObserver
import com.danifunker.misterkvms.core.ControllerState
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.core.MouseButton
import com.danifunker.misterkvms.core.NativeSurface
import com.danifunker.misterkvms.core.StreamLimits
import com.danifunker.misterkvms.core.demoHostProfile
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * UI-facing state for the app: the saved hosts and the one open session.
 * The session logic lives in the core's Controller (as on the iPhone); this
 * mirrors its state for Compose and adds what's Android-specific: the video
 * surface, touch input, the on-screen modifiers, audio focus and the app
 * lifecycle. Used on the main thread only.
 */
class AppModel(private val app: Application) {
    private val main = Handler(Looper.getMainLooper())
    val prefs = PrefStore(app)

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

    /** Function bar modifiers (HID usages): latched ones apply to the next
     *  key only, locked ones until tapped again. */
    var latched: Set<UShort> by mutableStateOf(emptySet())
        private set
    var locked: Set<UShort> by mutableStateOf(emptySet())
        private set

    /** The first-run screen: try the demo, or go on to the MiSTers. */
    var showWelcome by mutableStateOf(!prefs.bool(Pref.WELCOMED, false).value)
        private set

    /** Control given up from here: no input is sent (it would take control
     *  back) until the user takes control again. */
    var watching by mutableStateOf(false)
        private set

    /** "Screenshot copied" (or why not) over the picture for a moment. */
    var screenshotNotice: String? by mutableStateOf(null)
        private set

    /** The host that was open when the app went to the background. */
    private var suspendedId: String? = null

    private val demoModeState = prefs.bool(Pref.DEMO_MODE, false)
    private val keys = DeviceKeystore(app)

    init {
        loadKeys()
        // A touch screen captures as soon as a host connects (D6), whatever
        // the desktop apps' Capture automatically setting says.
        core.forceAutoCapture()
        if (showWelcome && profiles.any { !it.isDemo }) {
            // Set up before the first-run screen existed.
            prefs.bool(Pref.WELCOMED, false).value = true
            showWelcome = false
        }
        syncDemoHost()
        // Other viewers see this name ("Pixel 9 has control"). The core
        // falls back to "MiSTerKVMS" where it can't read a computer name.
        if (core.viewerName() == "MiSTerKVMS") core.setViewerName(defaultDeviceName())
        watchNetwork()
    }

    private fun handle(event: ControllerEvent) {
        when (event) {
            ControllerEvent.CHANGED -> state = core.state()
            ControllerEvent.PROFILES_CHANGED -> {
                profiles = core.profiles()
                state = core.state()
                saveMovedKeys()
            }
            // A touch screen is always "capturing" while connected (D6).
            ControllerEvent.CAPTURE_REQUESTED ->
                if (!someoneElseInControl && !watching) core.setCapturing(true)
            ControllerEvent.RELEASE_REQUESTED -> core.setCapturing(false)
            ControllerEvent.PREVIEWS_CHANGED -> {}
        }
    }

    // MARK: State

    val profile: HostProfile? get() = state.profileId?.let { id -> profiles.firstOrNull { it.id == id } }

    val isConnected: Boolean get() = state.connection is ConnectionState.Connected

    /** This phone's input reaches the MiSTer: connected to a capture server
     *  stream, not watching, nobody else in control. */
    val inControl: Boolean get() = state.stream != null && !watching && !someoneElseInControl

    /** A capture server stream that someone else controls. */
    val someoneElseInControl: Boolean
        get() = state.stream?.let { !it.haveControl && it.holder.isNotEmpty() } ?: false

    var muted: Boolean
        get() = state.muted
        set(value) = core.setMuted(value)

    // MARK: Hosts

    fun save(profile: HostProfile) = core.saveProfile(profile)

    fun delete(id: String) {
        val server = core.deviceKeyServer(id)
        core.deleteProfile(id)
        profiles = core.profiles()
        if (profiles.none { it.useInternet && core.deviceKeyServer(it.id) == server }) keys.delete(server)
    }

    // MARK: Demo (docs/APP-STORE.md S1)

    /** The Demo host is in the list: the test pattern, with the input it
     *  receives listed on screen instead of sent to a MiSTer. */
    var demoMode: Boolean
        get() = demoModeState.value
        set(value) {
            demoModeState.value = value
            if (!value && profile?.isDemo == true) close()
            syncDemoHost()
        }

    val isDemo: Boolean get() = profile?.isDemo == true

    /** The first-run screen's answer: `demo` turns demo mode on and opens it. */
    fun welcomed(demo: Boolean) {
        prefs.bool(Pref.WELCOMED, false).value = true
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

    // MARK: This phone's name

    /** Shown on the capture PC's list of paired devices and to other viewers
     *  ("Dani's Pixel has control"). */
    var deviceName: String
        get() = core.viewerName()
        set(value) = core.setViewerName(value.trim().ifEmpty { defaultDeviceName() })

    /** Still the generic name: ask for a real one before pairing. */
    val deviceNameIsDefault: Boolean get() = deviceName == defaultDeviceName()

    private fun defaultDeviceName(): String =
        Settings.Global.getString(app.contentResolver, Settings.Global.DEVICE_NAME)
            ?.takeIf { it.isNotBlank() } ?: Build.MODEL

    // MARK: Over the internet (docs/IPHONE.md §2–3)

    /** This phone holds a key for the MiSTer's capture PC. */
    fun isPaired(profile: HostProfile): Boolean = core.hasDeviceKey(profile.id)

    /** Use the MiSTer over the internet: reuses this phone's key for its
     *  capture PC, or pairs (on the home network) and keeps the new key in
     *  the Keystore. `again`: a new key, e.g. after the capture PC revoked
     *  the old one. Throws the server's own explanation, e.g. internet
     *  access turned off on the capture PC. */
    suspend fun pairForInternet(profile: HostProfile, again: Boolean = false) {
        val id = profile.id
        val name = deviceName
        val result = withContext(Dispatchers.IO) { core.pairDevice(id, name, again) }
        val have = keys.load(result.serverId)
        if (!result.reused || have == null) {
            if (!keys.save(DeviceKeystore.Entry(result.keyId, result.key), result.serverId)) {
                // Without the key saved, the next launch couldn't connect from outside.
                core.stopInternet(id)
                profiles = core.profiles()
                throw IllegalStateException("this phone couldn't keep the key in its Keystore")
            }
        }
        profiles = core.profiles()
    }

    /** Stop connecting this MiSTer from the internet. The key goes too when
     *  no other MiSTer of the same capture PC uses it. (The capture PC's
     *  setup page can revoke it there.) */
    fun stopInternet(profile: HostProfile) {
        val server = core.deviceKeyServer(profile.id)
        if (!core.stopInternet(profile.id)) keys.delete(server)
        profiles = core.profiles()
    }

    /** Keys from the Keystore, one per capture PC (the core never stores them). */
    private fun loadKeys() {
        for (profile in profiles.filter { it.useInternet }) {
            val server = core.deviceKeyServer(profile.id)
            keys.load(server)?.let { core.setDeviceKey(server, it.keyId, it.key) }
        }
    }

    /** A key kept by a capture PC's address moves to its id once the core
     *  learns the id: keep the Keystore in step. */
    private fun saveMovedKeys() {
        for (profile in profiles.filter { it.useInternet }) {
            val server = core.deviceKeyServer(profile.id)
            if (keys.load(server) == null) {
                core.deviceKey(server)?.let { keys.save(DeviceKeystore.Entry(it.keyId, it.key), server) }
            }
        }
    }

    // MARK: Predictive cursor

    /** For the route in use: the core keeps home's and the internet's apart. */
    var predictiveCursor: Boolean
        get() = state.display.predictiveCursor
        set(value) = core.setDisplay(state.display.copy(predictiveCursor = value))

    // MARK: Session

    fun open(profile: HostProfile) {
        if (profile.id == state.profileId) return
        latched = emptySet()
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

    /** Wi-Fi ↔ mobile data: open the MiSTer again, so the core picks the
     *  route (home or internet) for the new network. */
    private fun watchNetwork() {
        var current: Network? = null
        app.getSystemService(ConnectivityManager::class.java)
            .registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    val changed = current != null && current != network
                    current = network
                    if (changed) main.post { reconnect() }
                }
            })
    }

    private fun reconnect() {
        val p = profile ?: return
        if (p.isDemo) return
        core.releaseAll()
        core.closeHost()
        core.open(p.id)
        state = core.state()
    }

    // MARK: Video surface

    /** The SurfaceView's window (`AndroidCore.windowFromSurface`). */
    private var window = 0L

    /** The surface exists, or changed size: draw into it. */
    fun surfaceChanged(surface: Surface, widthPx: Int, heightPx: Int, density: Float) {
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

    /** No bigger picture than the phone shows: the server scales it down
     *  (1080p → 720p in portrait), a third of the data. */
    private fun askForScreenSize(widthPx: Int, heightPx: Int) {
        core.setStreamLimits(StreamLimits(widthPx.toUInt(), heightPx.toUInt(), null, null))
    }

    // MARK: Input

    /** Input from a viewer without control is dropped by the server; the
     *  first input on a free stream takes control (CAPTURE-SERVER.md §7). */
    private fun ready(): Boolean {
        if (!isConnected || someoneElseInControl || watching) return false
        if (!state.capturing) core.setCapturing(true)
        return true
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

    fun openMenu() {
        if (ready()) core.openMenu()
    }

    /** Text from the soft keyboard. */
    fun type(text: String) {
        if (ready()) core.typeText(text, takeModifiers())
    }

    /** A key from the function bar or the quick buttons, with the modifiers held. */
    fun tap(usage: UShort) {
        if (!ready()) return
        val modifiers = takeModifiers()
        if (modifiers.isEmpty()) core.tapKey(usage) else core.pressCombo(modifiers + usage)
    }

    /** The Type Text screen: `text` typed key by key at `keysPerSec`
     *  (remembered for the host by the core). */
    fun typeText(text: String, keysPerSec: Double) {
        if (ready()) core.typeTextAt(text, keysPerSec)
    }

    fun move(dx: Double, dy: Double) {
        if (ready()) core.mouseMove(dx, dy)
    }

    fun button(button: MouseButton, down: Boolean) {
        if (ready()) core.mouseButton(button, down)
    }

    fun click(button: MouseButton) {
        if (ready()) core.click(button)
    }

    fun zoom(factor: Double) = core.zoomBy(factor)
    fun resetZoom() = core.resetZoom()

    /** Pan by a fraction of the picture. */
    fun pan(dx: Double, dy: Double) = core.panBy(dx, dy)

    // MARK: Function bar modifiers

    enum class ModifierState { OFF, LATCHED, LOCKED }

    fun modifierState(usage: UShort): ModifierState = when (usage) {
        in locked -> ModifierState.LOCKED
        in latched -> ModifierState.LATCHED
        else -> ModifierState.OFF
    }

    /** Tap cycles off → next key only → locked → off. */
    fun cycleModifier(usage: UShort) {
        when (modifierState(usage)) {
            ModifierState.OFF -> latched = latched + usage
            ModifierState.LATCHED -> {
                latched = latched - usage
                locked = locked + usage
            }
            ModifierState.LOCKED -> locked = locked - usage
        }
    }

    private fun takeModifiers(): List<UShort> {
        val all = (latched + locked).sorted()
        latched = emptySet()
        return all
    }

    // MARK: Screenshots

    private var noticeGeneration = 0

    /** Screenshots get a bar with the core and the time. */
    var screenshotStamp: Boolean
        get() = core.screenshotStamp()
        set(value) = core.setScreenshotStamp(value)

    /** The picture as it is now, onto the clipboard as a PNG. */
    fun copyScreenshot() {
        val notice = try {
            val takenAt = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date())
            val shot = core.screenshot(takenAt)
            // The clipboard holds a link to the file, not the picture.
            val file = File(app.cacheDir, "screenshots/MiSTerKVMS screenshot.png")
            file.parentFile?.mkdirs()
            file.writeBytes(shot.png)
            val uri = FileProvider.getUriForFile(app, "${app.packageName}.screenshots", file)
            app.getSystemService(ClipboardManager::class.java)
                .setPrimaryClip(ClipData.newUri(app.contentResolver, "Screenshot", uri))
            "Screenshot copied"
        } catch (e: ControllerException.Failed) {
            "No screenshot: ${e.reason}"
        } catch (e: Exception) {
            "No screenshot: ${e.message}"
        }
        val generation = ++noticeGeneration
        screenshotNotice = notice
        main.postDelayed({ if (noticeGeneration == generation) screenshotNotice = null }, 2000)
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

/** What the core says went wrong, for an alert. */
fun describe(error: Throwable): String =
    (error as? ControllerException.Failed)?.reason ?: error.message ?: error.toString()
