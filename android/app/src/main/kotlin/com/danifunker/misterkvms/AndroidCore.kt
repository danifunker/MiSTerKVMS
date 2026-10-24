package com.danifunker.misterkvms

import android.content.Context
import android.view.Surface

/** The core's JNI functions (core/src/android.rs); the rest of the core is
 *  reached through the UniFFI bindings (com.danifunker.misterkvms.core). */
object AndroidCore {
    init {
        // The JVM looks JNI functions up only in libraries it loaded itself;
        // JNA opens the same library for the bindings.
        System.loadLibrary("kvms_core")
    }

    /** Once, before the Controller: the JVM and app context for audio, and
     *  where the settings go. */
    @JvmStatic external fun init(context: Context, filesDir: String)

    /** The `ANativeWindow*` behind a Surface (0 if none), for
     *  `NativeSurface.Android`. Release it after `detachSurface`. */
    @JvmStatic external fun windowFromSurface(surface: Surface): Long

    @JvmStatic external fun releaseWindow(window: Long)
}
