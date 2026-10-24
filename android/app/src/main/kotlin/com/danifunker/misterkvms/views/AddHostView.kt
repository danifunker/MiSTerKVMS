package com.danifunker.misterkvms.views

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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

/** Add a MiSTer by its stream's address (`host[:port]/stream`). Streams
 *  found on the network come in D2 (docs/ANDROID-HANDOFF.md). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddHostView(model: AppModel, onDone: () -> Unit) {
    var address by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf<String?>(null) }
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

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Add a MiSTer") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).padding(16.dp)) {
            Text("By address", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
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
