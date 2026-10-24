package com.danifunker.misterkvms

import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import com.danifunker.misterkvms.views.RootView

class MainActivity : ComponentActivity() {
    private val model get() = (application as MiSTerKVMSApp).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Always dark, like the iPhone app: light bar icons.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                RootView(model)
            }
        }
    }

    // Let go of the MiSTer while the app isn't on screen (releasing
    // control), and reconnect when it's back, as on the iPhone.
    override fun onStart() {
        super.onStart()
        model.resume()
    }

    override fun onStop() {
        model.suspend()
        super.onStop()
    }
}
