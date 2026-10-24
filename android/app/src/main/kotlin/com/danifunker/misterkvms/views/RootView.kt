package com.danifunker.misterkvms.views

import androidx.compose.runtime.Composable
import com.danifunker.misterkvms.model.AppModel

/** The first-run screen, the MiSTers, or the open one full screen. */
@Composable
fun RootView(model: AppModel) {
    when {
        model.state.profileId != null -> SessionScreen(model)
        model.showWelcome -> WelcomeView(model)
        else -> HostListView(model)
    }
}
