package com.danifunker.misterkvms.views

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.danifunker.misterkvms.model.AppModel

/** Asks for this phone's name (shown on the capture PC and to other
 *  viewers), then runs `then`. Starts with its current name (the phone's
 *  own, from Android's settings), so Continue keeps it. */
@Composable
fun NameThisPhone(model: AppModel, onCancel: () -> Unit, then: () -> Unit) {
    var name by rememberSaveable { mutableStateOf(model.deviceName) }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text("Name this phone") },
        text = {
            Column {
                Text("The capture PC lists paired phones by name, and other viewers see it when you have control.")
                OutlinedTextField(name, { name = it }, placeholder = { Text("Dani's Pixel") }, singleLine = true)
            }
        },
        confirmButton = {
            TextButton(
                {
                    model.deviceName = name
                    then()
                },
                enabled = name.isNotBlank(),
            ) { Text("Continue") }
        },
        dismissButton = { TextButton(onCancel) { Text("Cancel") } },
    )
}
