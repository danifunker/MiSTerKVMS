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
import androidx.compose.material3.Switch
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import com.danifunker.misterkvms.model.StreamBrowser
import com.danifunker.misterkvms.model.describe
import kotlinx.coroutines.launch
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.text.input.PasswordVisualTransformation
import com.danifunker.misterkvms.core.FoundJetKvm
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.core.findJetkvms
import com.danifunker.misterkvms.core.newHostProfile
import com.danifunker.misterkvms.model.JetKvmPasswords
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.streamName

/** Add a MiSTer: a stream found on the network, or one typed by address
 *  (`host[:port]/stream`) where discovery doesn't reach; or a JetKVM,
 *  straight from this phone (no capture server). */
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
    var useInternet by model.prefs.bool("add.useInternet", false)
    var settingUp by rememberSaveable { mutableStateOf<String?>(null) }
    var askingName by remember { mutableStateOf(false) }
    var pendingAdd by remember { mutableStateOf<(() -> Unit)?>(null) }
    /** Added, but the internet part failed: say so, then close. */
    var internetError by rememberSaveable { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    BackHandler(onBack = onDone)
    // A JetKVM directly: found on the network (a second's scan), or typed.
    val jetkvms = remember { mutableStateListOf<FoundJetKvm>() }
    var searchingJetkvms by remember { mutableStateOf(true) }
    var jetkvmAddress by rememberSaveable { mutableStateOf("") }
    var jetkvmPassword by remember { mutableStateOf("") }
    var jetkvmMister by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(Unit) {
        val found = withContext(Dispatchers.IO) { runCatching { findJetkvms() }.getOrDefault(emptyList()) }
        jetkvms.clear()
        jetkvms.addAll(found)
        searchingJetkvms = false
        if (jetkvmAddress.isEmpty() && found.size == 1 && found[0].isSetup) jetkvmAddress = found[0].address
    }

    fun addJetKvm() {
        val key = JetKvmPasswords.key(jetkvmAddress)
        if (key.isEmpty()) return
        val id = "jetkvm://$key"
        if (model.profiles.any { it.localVideo.equals(id, ignoreCase = true) }) {
            onDone()
            return
        }
        model.jetkvmPasswords.save(key, jetkvmPassword)
        val profile = newHostProfile().apply {
            source = "local"
            localVideo = id
            input = "auto"
            misterHost = jetkvmMister.trim()
            this.name = name.trim().ifEmpty { "MiSTer (JetKVM)" }
        }
        model.save(profile)
        onDone()
    }

    /** With the internet switch on, the capture PC lists this phone by its
     *  name: ask for a real one first. */
    fun whenNamed(add: () -> Unit) {
        if (useInternet && model.deviceNameIsDefault) {
            pendingAdd = add
            askingName = true
        } else {
            add()
        }
    }

    fun save(name: String, address: String, mister: String) {
        // The same address twice is one MiSTer: keep the saved one.
        if (model.profiles.any { it.stream.equals(address, ignoreCase = true) }) {
            onDone()
            return
        }
        val profile = makeProfile(address).apply {
            this.name = name
            misterHost = mister
        }
        model.save(profile)
        if (!useInternet) {
            onDone()
            return
        }
        settingUp = name
        scope.launch {
            try {
                model.pairForInternet(profile)
                onDone()
            } catch (e: Exception) {
                internetError = describe(e)
            }
            settingUp = null
        }
    }

    fun addTyped() {
        var text = address.trim()
        if (!text.contains("://")) text = "mkvm://$text"
        val profile = makeProfile(text)
        if (profile.streamName.isEmpty()) {
            error = "Add the stream's name after the address: pc/MiSTer-SS1."
            return
        }
        val given = name.trim()
        whenNamed { save(given.ifEmpty { profile.streamName }, text, "") }
    }

    /** Already in the list (same stream name on the same capture server). */
    fun isSaved(stream: StreamBrowser.Found) =
        model.profiles.any { it.streamName == stream.name && (stream.mister.isEmpty() || it.misterHost == stream.mister) }

    fun add(stream: StreamBrowser.Found) = whenNamed { save(stream.name, stream.address, stream.mister) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Add a MiSTer") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).verticalScroll(rememberScrollState()).padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Also use over the internet", Modifier.weight(1f))
                Switch(useInternet, { useInternet = it }, enabled = settingUp == null)
            }
            settingUp?.let {
                Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("Setting up $it for the internet…", color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 12.dp))
                }
            }
            Text(
                "Pairs this phone with the capture PC (once per capture PC), so the MiSTer works from anywhere, " +
                    "encrypted. Needs “Allow connections from the internet” on the capture PC's setup page.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = 24.dp),
            )

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
                    Modifier.fillMaxWidth().clickable(enabled = !saved && settingUp == null) { add(stream) }.padding(vertical = 10.dp),
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
            Button(::addTyped, Modifier.padding(top = 16.dp), enabled = address.isNotBlank() && settingUp == null) { Text("Add") }

            Text("JetKVM, directly", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 24.dp))
            if (searchingJetkvms) {
                Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("Looking for JetKVMs…", color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 12.dp))
                }
            }
            jetkvms.forEach { found ->
                Row(
                    Modifier.fillMaxWidth().clickable(enabled = found.isSetup) { jetkvmAddress = found.address }.padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(found.name, Modifier.weight(1f))
                    if (JetKvmPasswords.key(jetkvmAddress) == found.address) Icon(Icons.Filled.Check, "Chosen")
                }
            }
            OutlinedTextField(
                jetkvmAddress, { jetkvmAddress = it },
                Modifier.fillMaxWidth().padding(top = 8.dp),
                placeholder = { Text("JetKVM address, e.g. 192.168.1.60") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
            )
            OutlinedTextField(
                jetkvmPassword, { jetkvmPassword = it },
                Modifier.fillMaxWidth().padding(top = 8.dp),
                placeholder = { Text("Password (if it has one)") },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
            )
            OutlinedTextField(
                jetkvmMister, { jetkvmMister = it },
                Modifier.fillMaxWidth().padding(top = 8.dp),
                placeholder = { Text("MiSTer address (optional)") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
            )
            Text(
                "Watch and control a MiSTer through its JetKVM without a capture server: its USB is the MiSTer's " +
                    "keyboard and mouse. One viewer at a time, at home only. The password is encrypted with a key in " +
                    "this phone's Keystore. With the MiSTer's address (mrext Remote), the app also shows which core " +
                    "is running. A JetKVM Mini needs the MiSTer at 1080p 30 Hz and has no sound.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )
            Button(::addJetKvm, Modifier.padding(top = 16.dp), enabled = JetKvmPasswords.key(jetkvmAddress).isNotEmpty()) {
                Text("Add JetKVM")
            }
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
    internetError?.let {
        AlertDialog(
            onDismissRequest = { internetError = null; onDone() },
            confirmButton = { TextButton({ internetError = null; onDone() }) { Text("OK") } },
            title = { Text("Added, without the internet") },
            text = { Text("$it\n\nTry again later from the MiSTer's settings (long-press it in the list).") },
        )
    }
    if (askingName) {
        NameThisPhone(model, onCancel = { askingName = false; pendingAdd = null }) {
            askingName = false
            pendingAdd?.invoke()
            pendingAdd = null
        }
    }
}

private fun makeProfile(address: String): HostProfile =
    newHostProfile().apply {
        source = "capture"
        stream = address
    }
