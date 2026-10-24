package com.danifunker.misterkvms.views

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Monitor
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.foundation.background
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.isDemo
import com.danifunker.misterkvms.model.subtitle

/** The MiSTers (one per capture server stream). Tapping one opens it full
 *  screen; one at a time (P13). Long-press for its actions. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostListView(model: AppModel) {
    var adding by rememberSaveable { mutableStateOf(false) }
    var appSettings by rememberSaveable { mutableStateOf(false) }
    var settingsFor by rememberSaveable { mutableStateOf<String?>(null) }
    when {
        adding -> return AddHostView(model, onDone = { adding = false })
        appSettings -> return AppSettingsView(model, onDone = { appSettings = false })
        settingsFor != null -> return HostSettingsView(model, settingsFor!!, onDone = { settingsFor = null })
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("MiSTerKVMS") },
                navigationIcon = {
                    IconButton({ appSettings = true }) { Icon(Icons.Outlined.Settings, "Settings") }
                },
                actions = {
                    IconButton({ adding = true }) { Icon(Icons.Filled.Add, "Add a MiSTer") }
                },
            )
        },
    ) { padding ->
        if (model.profiles.isEmpty()) {
            NoMiSTers(model, onAdd = { adding = true }, Modifier.padding(padding))
            return@Scaffold
        }
        LazyColumn(Modifier.padding(padding).fillMaxSize()) {
            // The Demo host first, apart from the real MiSTers.
            items(model.profiles.filter { it.isDemo }, key = { it.id }) { profile ->
                HostRow(profile, demo = true, onOpen = { model.open(profile) }) { close ->
                    DropdownMenuItem(
                        text = { Text("Hide the demo") },
                        leadingIcon = { Icon(Icons.Outlined.VisibilityOff, null) },
                        onClick = { close(); model.demoMode = false },
                    )
                }
            }
            items(model.profiles.filter { !it.isDemo }, key = { it.id }) { profile ->
                HostRow(profile, demo = false, onOpen = { model.open(profile) }) { close ->
                    DropdownMenuItem(
                        text = { Text("Settings") },
                        leadingIcon = { Icon(Icons.Outlined.Settings, null) },
                        onClick = { close(); settingsFor = profile.id },
                    )
                    DropdownMenuItem(
                        text = { Text("Delete") },
                        leadingIcon = { Icon(Icons.Outlined.Delete, null) },
                        onClick = { close(); model.delete(profile.id) },
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HostRow(
    profile: HostProfile,
    demo: Boolean,
    onOpen: () -> Unit,
    menu: @Composable (close: () -> Unit) -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .combinedClickable(onClick = onOpen, onLongClick = { menuOpen = true })
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(profile.name, style = MaterialTheme.typography.titleMedium)
                Text(profile.subtitle, style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (demo) {
                Text("DEMO", style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold,
                    modifier = Modifier
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.25f), CircleShape)
                        .padding(horizontal = 6.dp, vertical = 2.dp))
            }
        }
        DropdownMenu(menuOpen, { menuOpen = false }) { menu { menuOpen = false } }
    }
    HorizontalDivider()
}

@Composable
private fun NoMiSTers(model: AppModel, onAdd: () -> Unit, modifier: Modifier) {
    Column(
        modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(Icons.Outlined.Monitor, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text("No MiSTers yet", style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(top = 12.dp))
        Text(
            "You need a MiSTer FPGA running mrext Remote, a capture card showing it, and MiSTerKVMS Capture " +
                "(Windows, Mac or Linux) on the computer the card is plugged into. Then add its stream here " +
                "while you're on the same Wi-Fi.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = 8.dp),
        )
        Button(onAdd, Modifier.padding(top = 16.dp)) { Text("Add a MiSTer") }
        TextButton({ model.demoMode = true }) { Text("Try it without a MiSTer") }
    }
}
