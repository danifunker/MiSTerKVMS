package com.danifunker.misterkvms.views

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.BuildConfig
import com.danifunker.misterkvms.model.AppModel

/** Settings for this phone (not one MiSTer): its name, demo mode, and
 *  About (the version and credits, as in the desktop apps). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppSettingsView(model: AppModel, onDone: () -> Unit) {
    var name by rememberSaveable { mutableStateOf(if (model.deviceNameIsDefault) "" else model.deviceName) }
    var demo by rememberSaveable { mutableStateOf(model.demoMode) }
    var stamp by rememberSaveable { mutableStateOf(model.screenshotStamp) }
    val context = LocalContext.current
    BackHandler(onBack = onDone)

    fun save() {
        model.deviceName = name
        if (demo != model.demoMode) model.demoMode = demo
        if (stamp != model.screenshotStamp) model.screenshotStamp = stamp
        onDone()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
                actions = { TextButton(::save) { Text("Done") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            Heading("This phone's name")
            OutlinedTextField(
                name, { name = it },
                Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                placeholder = { Text(android.os.Build.MODEL) },
                singleLine = true,
            )
            Note(
                "Capture PCs list this phone by this name under their paired devices, and other viewers see it " +
                    "when you have control (“${name.ifEmpty { android.os.Build.MODEL }} has control”). A change " +
                    "shows on the capture PC the next time this phone connects.",
            )

            SwitchRow("Demo mode", demo) { demo = it }
            Note(
                "Adds a Demo host to the list: a test pattern with sound instead of a MiSTer, and everything you " +
                    "type or tap listed on screen. For trying the app without a MiSTer; your MiSTers aren't affected.",
            )

            SwitchRow("Core and time on screenshots", stamp) { stamp = it }
            Note(
                "The screenshot button copies the picture. With this on, a bar under it says which core was " +
                    "running and when.",
            )

            Heading("About")
            Credit("Version", "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
            Credit("MiSTer-Kun mascot",
                "Original design by HeWhoisRed, a gift to the MiSTer community. Remastered artwork by baxysquare/mister_kun.")
            Credit("MiSTer FPGA", "MiSTer FPGA project by Sorgelig and contributors.")
            Credit("Keyboard & mouse", "mrext Remote by wizzo and contributors.")
            Credit("Built with", "Rust: wgpu, cpal, tungstenite, UniFFI. Kotlin: Jetpack Compose.")
            TextButton(
                { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/danifunker/MiSTerKVMS"))) },
                Modifier.padding(horizontal = 8.dp),
            ) { Text("Source on GitHub (MIT License)") }
        }
    }
}

@Composable
fun Heading(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 8.dp))
}

@Composable
fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
}

@Composable
fun SwitchRow(title: String, on: Boolean, set: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { set(!on) }.padding(horizontal = 16.dp, vertical = 8.dp).padding(top = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(title)
        Switch(on, set)
    }
}

@Composable
private fun Credit(title: String, text: String) {
    Column(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
        Text(title, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
