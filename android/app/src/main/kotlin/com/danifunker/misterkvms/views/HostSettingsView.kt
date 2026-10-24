package com.danifunker.misterkvms.views

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.model.AppModel

/** One MiSTer's settings: its name. Using it over the internet comes with
 *  D3 (docs/ANDROID-HANDOFF.md). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostSettingsView(model: AppModel, hostId: String, onDone: () -> Unit) {
    val profile = model.profiles.firstOrNull { it.id == hostId }
    var name by rememberSaveable { mutableStateOf(profile?.name ?: "") }
    BackHandler(onBack = onDone)

    fun save() {
        profile?.let { p ->
            name.trim().takeIf { it.isNotEmpty() }?.let { p.name = it }
            model.save(p)
        }
        onDone()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(profile?.name ?: "MiSTer") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
                actions = { TextButton(::save) { Text("Done") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding)) {
            Heading("Name")
            OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp), singleLine = true)
        }
    }
}
