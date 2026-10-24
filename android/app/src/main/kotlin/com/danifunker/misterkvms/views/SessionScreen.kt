package com.danifunker.misterkvms.views

import android.app.Activity
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.danifunker.misterkvms.core.ConnectionState
import com.danifunker.misterkvms.model.AppModel

/** One MiSTer, full screen (P13): the picture, its sound, and the buttons
 *  over it. Touch input, the keyboard and the quick buttons come in D2. */
@Composable
fun SessionScreen(model: AppModel) {
    BackHandler { model.close() }
    HideSystemBars()

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        VideoSurface(model, Modifier.fillMaxSize())

        StatusOverlay(model, Modifier.align(Alignment.Center))

        Column(
            Modifier.safeDrawingPadding().padding(horizontal = 12.dp).padding(top = 4.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    model.profile?.name ?: "MiSTer",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = Color.White,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier
                        .background(Color.Black.copy(alpha = 0.5f), CircleShape)
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                )
                Spacer(Modifier.weight(1f))
                RoundButton(
                    if (model.muted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
                    if (model.muted) "Unmute" else "Mute",
                ) { model.muted = !model.muted }
                RoundButton(Icons.Filled.Close, "Disconnect") { model.close() }
            }
            if (model.isDemo) DemoInputList(model)
        }
    }
}

/** The picture: the core draws into this SurfaceView's window. Its surface
 *  goes and comes back (background, rotation) without the connection. */
@Composable
private fun VideoSurface(model: AppModel, modifier: Modifier) {
    AndroidView(
        factory = { context ->
            SurfaceView(context).apply {
                keepScreenOn = true
                holder.addCallback(object : SurfaceHolder.Callback {
                    override fun surfaceCreated(holder: SurfaceHolder) {}

                    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                        model.surfaceChanged(holder.surface, width, height, resources.displayMetrics.density)
                    }

                    override fun surfaceDestroyed(holder: SurfaceHolder) = model.surfaceDestroyed()
                })
            }
        },
        modifier = modifier,
    )
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
private fun RoundButton(icon: ImageVector, label: String, onClick: () -> Unit) {
    IconButton(
        onClick,
        Modifier.padding(start = 6.dp).size(40.dp).background(Color.Black.copy(alpha = 0.5f), CircleShape),
    ) {
        Icon(icon, label, tint = Color.White, modifier = Modifier.height(20.dp))
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
