package com.danifunker.misterkvms

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.core.view.WindowCompat
import com.danifunker.misterkvms.views.RootView

class MainActivity : ComponentActivity() {
    private val model get() = (application as MiSTerKVMSApp).model

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Edge to edge: Android 15 and later do it for every app targeting
        // them; before that, the theme makes the bars transparent and this
        // lets the app draw behind them. (Not androidx's enableEdgeToEdge(),
        // which still calls the Window APIs Android 15 deprecates.)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        // Always dark, like the iPhone app: light bar icons.
        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }
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
