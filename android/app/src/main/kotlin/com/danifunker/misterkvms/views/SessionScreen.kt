package com.danifunker.misterkvms.views

import android.app.Activity
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.SportsEsports
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.Screenshot
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.danifunker.misterkvms.core.ConnectionState
import com.danifunker.misterkvms.input.FunctionBar
import com.danifunker.misterkvms.input.KeyboardHost
import com.danifunker.misterkvms.input.TouchpadView
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.Pref
import java.util.Locale

/** One MiSTer, full screen (P13): the video as a touchpad, the keyboard and
 *  function bar, the quick buttons, the control banner. Each can be hidden
 *  from the controls menu (the corner button, or a three-finger tap), P14,
 *  and the quick buttons can be moved anywhere (Arrange mode). */
@Composable
fun SessionScreen(model: AppModel) {
    val showKeyboard = model.prefs.bool(Pref.KEYBOARD, true)
    val showFunctionBar by model.prefs.bool(Pref.FUNCTION_BAR, true)
    val showBanner by model.prefs.bool(Pref.BANNER, true)
    val showStats by model.prefs.bool(Pref.STATS, false)
    var menuOpen by rememberSaveable { mutableStateOf(false) }
    var switcherOpen by rememberSaveable { mutableStateOf(false) }
    var arranging by rememberSaveable { mutableStateOf(false) }
    val sheetOpen = menuOpen || switcherOpen

    BackHandler {
        if (arranging) arranging = false else model.close()
    }
    HideSystemBars()

    // The keyboard and the bar push the picture and touchpad up.
    Column(Modifier.fillMaxSize().background(Color.Black).windowInsetsPadding(WindowInsets.ime)) {
        Box(Modifier.fillMaxWidth().weight(1f)) {
            AndroidView(
                factory = { context -> TouchpadView(context, model) },
                update = { view ->
                    view.onMenu = { menuOpen = !menuOpen }
                    view.onSwitch = { model.step(it) }
                },
                modifier = Modifier.fillMaxSize(),
            )

            StatusOverlay(model, Modifier.align(Alignment.Center))

            QuickButtons(model, arranging, Modifier.safeDrawingPadding())

            if (arranging) {
                ArrangeBar(Modifier.safeDrawingPadding()) { arranging = false }
            } else {
                Column(
                    Modifier.safeDrawingPadding().padding(horizontal = 12.dp).padding(top = 4.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    TopBar(model, onSwitcher = { switcherOpen = true }, onMenu = { menuOpen = true })
                    if (showBanner) ControlBanner(model)
                    AnimatedVisibility(model.screenshotNotice != null, enter = fadeIn(), exit = fadeOut()) {
                        Pill(model.screenshotNotice ?: "")
                    }
                    if (showStats) Stats(model)
                    if (model.isDemo) DemoInputList(model)
                }
            }
        }
        if (showFunctionBar && !sheetOpen) FunctionBar(model)
    }

    KeyboardHost(model, showKeyboard, covered = sheetOpen || arranging)

    if (menuOpen) ControlsMenu(model, onDismiss = { menuOpen = false }, arrange = { arranging = true })
    if (switcherOpen) MiSTerSwitcher(model, onDismiss = { switcherOpen = false })
}

@Composable
private fun TopBar(model: AppModel, onSwitcher: () -> Unit, onMenu: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        // The open MiSTer's name: tap to switch to another one or disconnect.
        Row(
            Modifier
                .widthIn(max = 200.dp)
                .height(40.dp)
                .background(Color.Black.copy(alpha = 0.5f), CircleShape)
                .clickable(onClick = onSwitcher)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (model.state.stream?.internet == true) {
                Icon(Icons.Outlined.Language, "Over the internet", tint = Color.White, modifier = Modifier.size(14.dp))
            }
            Text(model.profile?.name ?: "MiSTer", color = Color.White, fontWeight = FontWeight.SemiBold,
                style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false))
            Icon(Icons.Filled.KeyboardArrowDown, "Switch MiSTer or disconnect", tint = Color.White,
                modifier = Modifier.size(16.dp))
        }
        Spacer(Modifier.weight(1f))
        // Filled: this phone controls the MiSTer, tap to give control up.
        // Outline: watching, tap to take control (CAPTURE-SERVER.md §7).
        if (model.state.stream?.connected == true) {
            RoundButton(
                if (model.inControl) Icons.Filled.SportsEsports else Icons.Outlined.SportsEsports,
                if (model.inControl) "Release control" else "Take control",
                tint = if (model.inControl) MaterialTheme.colorScheme.primary else Color.White,
            ) { if (model.inControl) model.releaseControl() else model.takeControl() }
        }
        RoundButton(Icons.Outlined.Screenshot, "Copy screenshot") { model.copyScreenshot() }
        RoundButton(
            if (model.muted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
            if (model.muted) "Unmute" else "Mute",
        ) { model.muted = !model.muted }
        RoundButton(Icons.Filled.Tune, "Controls", onClick = onMenu)
        RoundButton(Icons.Filled.Close, "Disconnect") { model.close() }
    }
}

/** "Name has control" with Take control (CAPTURE-SERVER.md §7), or why the
 *  MiSTer can't be reached. */
@Composable
private fun ControlBanner(model: AppModel) {
    val stream = model.state.stream ?: return
    if (!stream.connected) return
    if (model.someoneElseInControl || model.watching) {
        Row(
            Modifier.background(Color.Black.copy(alpha = 0.7f), CircleShape).padding(start = 12.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(if (stream.holder.isEmpty()) "Watching: nobody has control" else "${stream.holder} has control",
                color = Color.White, style = MaterialTheme.typography.bodySmall)
            Button({ model.takeControl() }, contentPadding = ButtonDefaults.TextButtonContentPadding) {
                Text("Take control", fontSize = 13.sp)
            }
        }
    } else if (!stream.misterConnected) {
        Pill("The capture server can't reach the MiSTer")
    }
}

@Composable
private fun Stats(model: AppModel) {
    val v = model.state.videoStats
    val parts = listOfNotNull(
        v?.let { "${it.sourceWidth}×${it.sourceHeight}" },
        v?.let { String.format(Locale.US, "%.0f fps", it.receivedFps) },
        v?.let { String.format(Locale.US, "%.0f ms", it.frameAgeMs) },
        model.state.stream?.let { String.format(Locale.US, "rtt %.0f ms", it.roundTripMs) },
    )
    if (parts.isNotEmpty()) Pill(parts.joinToString("  ·  "), monospace = true)
}

/** Arrange mode's instructions and Done. */
@Composable
private fun ArrangeBar(modifier: Modifier, onDone: () -> Unit) {
    Row(
        modifier
            .padding(horizontal = 12.dp, vertical = 4.dp)
            .fillMaxWidth()
            .background(Color.Black.copy(alpha = 0.8f), RoundedCornerShape(14.dp))
            .padding(horizontal = 14.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Drag the buttons where you want them", color = Color.White, modifier = Modifier.weight(1f))
        Button(onDone) { Text("Done") }
    }
}

/** Connecting, or why there's no picture. */
@Composable
private fun StatusOverlay(model: AppModel, modifier: Modifier) {
    val state = model.state
    val text = when (val connection = state.connection) {
        is ConnectionState.Connecting -> "Connecting to ${model.profile?.name ?: "MiSTer"}…"
        is ConnectionState.Disconnected -> connection.reason.ifEmpty { "Not connected" }
        is ConnectionState.Connected -> {
            val stream = state.stream
            if (stream != null && !stream.connected) {
                stream.error.ifEmpty { "Connecting to the capture server…" }
            } else {
                state.videoError
            }
        }
    } ?: return
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        color = Color.White,
        textAlign = TextAlign.Center,
        modifier = modifier
            .padding(horizontal = 40.dp)
            .background(Color.Black.copy(alpha = 0.7f), RoundedCornerShape(10.dp))
            .padding(12.dp),
    )
}

@Composable
private fun Pill(text: String, monospace: Boolean = false) {
    Text(
        text,
        color = Color.White,
        style = MaterialTheme.typography.bodySmall,
        fontFamily = if (monospace) FontFamily.Monospace else null,
        modifier = Modifier.background(Color.Black.copy(alpha = 0.7f), CircleShape).padding(horizontal = 12.dp, vertical = 6.dp),
    )
}

@Composable
private fun RoundButton(icon: ImageVector, label: String, tint: Color = Color.White, onClick: () -> Unit) {
    IconButton(
        onClick,
        Modifier.padding(start = 6.dp).size(40.dp).background(Color.Black.copy(alpha = 0.5f), CircleShape),
    ) {
        Icon(icon, label, tint = tint, modifier = Modifier.size(20.dp))
    }
}

/** Full screen while a MiSTer is open: the bars come back with a swipe. */
@Composable
private fun HideSystemBars() {
    val view = LocalView.current
    DisposableEffect(view) {
        val window = (view.context as Activity).window
        val bars = WindowCompat.getInsetsController(window, view)
        bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        bars.hide(WindowInsetsCompat.Type.systemBars())
        onDispose { bars.show(WindowInsetsCompat.Type.systemBars()) }
    }
}
