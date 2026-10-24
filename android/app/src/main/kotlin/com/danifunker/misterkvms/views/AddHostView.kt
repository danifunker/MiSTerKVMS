package com.danifunker.misterkvms.views

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import com.danifunker.misterkvms.model.StreamBrowser
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.core.newHostProfile
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.streamName

/** Add a MiSTer: a stream found on the network, or one typed by address
 *  (`host[:port]/stream`) where discovery doesn't reach. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddHostView(model: AppModel, onDone: () -> Unit) {
    var address by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val browser = remember { StreamBrowser(context) }
    DisposableEffect(browser) {
        browser.start()
        onDispose { browser.stop() }
    }
    BackHandler(onBack = onDone)

    fun addTyped() {
        var text = address.trim()
        if (!text.contains("://")) text = "mkvm://$text"
        val profile = makeProfile(text)
        if (profile.streamName.isEmpty()) {
            error = "Add the stream's name after the address: pc/MiSTer-SS1."
            return
        }
        // The same address twice is one MiSTer: keep the saved one.
        if (model.profiles.none { it.stream.equals(text, ignoreCase = true) }) {
            profile.name = name.trim().ifEmpty { profile.streamName }
            model.save(profile)
        }
        onDone()
    }

    /** Already in the list (same stream name on the same capture server). */
    fun isSaved(stream: StreamBrowser.Found) =
        model.profiles.any { it.streamName == stream.name && (stream.mister.isEmpty() || it.misterHost == stream.mister) }

    fun add(stream: StreamBrowser.Found) {
        // The same address twice is one MiSTer: keep the saved one.
        if (model.profiles.none { it.stream.equals(stream.address, ignoreCase = true) }) {
            model.save(makeProfile(stream.address).apply {
                name = stream.name
                misterHost = stream.mister
            })
        }
        onDone()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Add a MiSTer") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).verticalScroll(rememberScrollState()).padding(16.dp)) {
            Text("On this network", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
            if (browser.found.isEmpty()) {
                Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("Looking for capture servers…", color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 12.dp))
                }
            }
            browser.found.forEach { stream ->
                val saved = isSaved(stream)
                Row(
                    Modifier.fillMaxWidth().clickable(enabled = !saved) { add(stream) }.padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(stream.name)
                        Text(if (stream.mister.isEmpty()) stream.server else "${stream.mister} on ${stream.server}",
                            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (saved) Icon(Icons.Filled.Check, "Added", tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }

            Text("By address", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 24.dp))
            OutlinedTextField(
                address, { address = it },
                Modifier.fillMaxWidth().padding(top = 8.dp),
                placeholder = { Text("capture-pc/MiSTer-SS1") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
            )
            OutlinedTextField(
                name, { name = it },
                Modifier.fillMaxWidth().padding(top = 8.dp),
                placeholder = { Text("Name (optional)") },
                singleLine = true,
            )
            Text(
                "The capture server's address and the stream name, as on its setup page. Add the port if it " +
                    "isn't 5970: pc:6000/MiSTer-SS1.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )
            Button(::addTyped, Modifier.padding(top = 16.dp), enabled = address.isNotBlank()) { Text("Add") }
        }
    }

    error?.let {
        AlertDialog(
            onDismissRequest = { error = null },
            confirmButton = { TextButton({ error = null }) { Text("OK") } },
            title = { Text("Can't add it") },
            text = { Text(it) },
        )
    }
}

private fun makeProfile(address: String): HostProfile =
    newHostProfile().apply {
        source = "capture"
        stream = address
    }
