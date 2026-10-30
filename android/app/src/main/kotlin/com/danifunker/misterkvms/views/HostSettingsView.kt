package com.danifunker.misterkvms.views

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.danifunker.misterkvms.core.HostProfile
import com.danifunker.misterkvms.model.AppModel
import com.danifunker.misterkvms.model.describe
import kotlinx.coroutines.launch

/** One MiSTer's settings: its name, and using it over the internet (this
 *  phone's key for its capture PC, the address from outside). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HostSettingsView(model: AppModel, hostId: String, onDone: () -> Unit) {
    val profile = model.profiles.firstOrNull { it.id == hostId }
    var name by rememberSaveable { mutableStateOf(profile?.name ?: "") }
    var internetAddress by rememberSaveable { mutableStateOf(profile?.internetAddress ?: "") }
    var pairing by rememberSaveable { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf<String?>(null) }
    var askingName by rememberSaveable { mutableStateOf(false) }
    var pendingAgain by rememberSaveable { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    BackHandler(onBack = onDone)

    // Pairing brings the address the capture PC knows.
    LaunchedEffect(profile?.internetAddress) {
        val address = profile?.internetAddress
        if (!address.isNullOrEmpty() && internetAddress.isEmpty()) internetAddress = address
    }

    fun startPairing(p: HostProfile, again: Boolean) {
        error = null
        // The capture PC lists this phone by its name: ask for a real one first.
        if (model.deviceNameIsDefault && !askingName) {
            pendingAgain = again
            askingName = true
            return
        }
        pairing = true
        scope.launch {
            try {
                model.pairForInternet(p, again)
            } catch (e: Exception) {
                error = "Couldn't set it up: ${describe(e)}"
            }
            pairing = false
        }
    }

    fun save() {
        profile?.let { p ->
            name.trim().takeIf { it.isNotEmpty() }?.let { p.name = it }
            p.internetAddress = internetAddress.trim()
            model.save(p)
        }
        onDone()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(profile?.name ?: "MiSTer") },
                navigationIcon = { IconButton(onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") } },
                actions = { TextButton(::save, enabled = !pairing) { Text("Done") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).verticalScroll(rememberScrollState()).padding(bottom = 24.dp)) {
            Heading("Name")
            OutlinedTextField(name, { name = it }, Modifier.fillMaxWidth().padding(horizontal = 16.dp), singleLine = true)

            if (profile != null) {
                Heading("Over the internet")
                SwitchRow("Use over the internet", profile.useInternet || pairing) { on ->
                    if (pairing) return@SwitchRow
                    error = null
                    if (on) startPairing(profile, again = false) else model.stopInternet(profile)
                }
                if (pairing) {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                        Text("Setting up with the capture PC…", color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(start = 12.dp))
                    }
                } else if (profile.useInternet) {
                    val paired = model.isPaired(profile)
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(if (paired) Icons.Filled.Lock else Icons.Filled.Warning, null, Modifier.size(16.dp),
                            tint = if (paired) MaterialTheme.colorScheme.onSurfaceVariant else Color(0xFFFF9F0A))
                        Text(
                            if (paired) "Paired: encrypted with this phone's key for this capture PC" else "Not paired on this phone",
                            style = MaterialTheme.typography.bodySmall,
                            color = if (paired) MaterialTheme.colorScheme.onSurfaceVariant else Color(0xFFFF9F0A),
                            modifier = Modifier.padding(start = 8.dp),
                        )
                    }
                    OutlinedTextField(
                        internetAddress, { internetAddress = it },
                        Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                        placeholder = { Text("myhome.duckdns.org:5970") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(
                            capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri),
                    )
                    TextButton({ startPairing(profile, again = true) }, Modifier.padding(horizontal = 8.dp)) { Text("Pair again") }
                }
                error?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.padding(horizontal = 16.dp))
                }
                Note(
                    if (profile.useInternet) {
                        "Away from home the app connects to this address (your home's public address or dynamic-DNS " +
                            "name, and the port forwarded to the capture PC). At home it connects directly. Pair again " +
                            "if the capture PC revoked this phone.\n\nTurning this off (or deleting this MiSTer) also " +
                            "removes this phone from the capture PC. To see the other devices paired with it and remove " +
                            "them, open http://localhost:5971 on the capture PC."
                    } else {
                        "Lets this phone connect from anywhere, encrypted. It pairs with the capture PC while you're on " +
                            "the same Wi-Fi (once per capture PC: its other MiSTers use the same key). First turn on " +
                            "“Allow connections from the internet” on the capture PC's setup page."
                    },
                )
            }
        }
    }

    if (askingName && profile != null) {
        NameThisPhone(model, onCancel = { askingName = false }) {
            askingName = false
            startPairing(profile, pendingAgain)
        }
    }
}
