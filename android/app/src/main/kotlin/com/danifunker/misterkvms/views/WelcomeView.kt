package com.danifunker.misterkvms.views

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Cable
import androidx.compose.material.icons.outlined.DesktopWindows
import androidx.compose.material.icons.outlined.Memory
import androidx.compose.material.icons.outlined.Monitor
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.model.AppModel
import kotlinx.coroutines.delay

/** The first-run screen (docs/APP-STORE.md S1): what the app needs, and a
 *  way to try it without any of that. Shown once, and only to a phone with
 *  no MiSTers saved. */
@Composable
fun WelcomeView(model: AppModel) {
    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.safeDrawingPadding().padding(horizontal = 24.dp).padding(bottom = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Spacer(Modifier.weight(1f))
            Icon(Icons.Outlined.Monitor, null, Modifier.size(56.dp), tint = MaterialTheme.colorScheme.primary)
            Text("MiSTerKVMS", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold, modifier = Modifier.padding(top = 16.dp))
            Text(
                "Your MiSTer's screen and sound on your phone, with its keyboard and mouse under your fingers.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 8.dp, start = 8.dp, end = 8.dp),
            )
            Column(Modifier.padding(top = 32.dp, start = 8.dp, end = 8.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Need(Icons.Outlined.Memory, "A MiSTer FPGA running mrext Remote")
                Need(Icons.Outlined.Cable, "A capture card showing its HDMI output")
                Need(Icons.Outlined.DesktopWindows, "MiSTerKVMS Capture on the computer the card is plugged into (Windows, Mac or Linux)")
            }
            Spacer(Modifier.weight(1f))
            Button({ model.welcomed(demo = false) }, Modifier.fillMaxWidth()) {
                Text("I have MiSTerKVMS Capture set up")
            }
            OutlinedButton({ model.welcomed(demo = true) }, Modifier.fillMaxWidth().padding(top = 4.dp)) {
                Text("Try it without a MiSTer")
            }
            Text(
                "Shows a test pattern and lists what you type and tap. Turn it off in Settings.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}

@Composable
private fun Need(icon: ImageVector, text: String) {
    Row(verticalAlignment = Alignment.Top) {
        Icon(icon, null, Modifier.width(30.dp), tint = MaterialTheme.colorScheme.primary)
        Text(text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(start = 14.dp))
    }
}

/** A demo host's input over the picture: what a MiSTer would have
 *  received, newest last, each line gone after 5 seconds (the core keeps
 *  the list). */
@Composable
fun DemoInputList(model: AppModel, modifier: Modifier = Modifier) {
    var lines by remember { mutableStateOf(emptyList<String>()) }
    LaunchedEffect(Unit) {
        while (true) {
            lines = model.core.demoInput()
            delay(150)
        }
    }
    Column(
        modifier
            .background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(10.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text("Demo: what the MiSTer would receive", style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold, color = Color.White.copy(alpha = 0.7f))
        if (lines.isEmpty()) {
            Text("Tap, drag or type…", style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace, color = Color.White.copy(alpha = 0.7f))
        } else {
            lines.forEach {
                Text(it, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, color = Color.White)
            }
        }
    }
}
