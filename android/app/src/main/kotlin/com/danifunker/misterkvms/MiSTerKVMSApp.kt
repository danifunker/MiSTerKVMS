package com.danifunker.misterkvms

import android.app.Application
import com.danifunker.misterkvms.model.AppModel

/** Owns the one AppModel, so a recreated activity finds the open session. */
class MiSTerKVMSApp : Application() {
    lateinit var model: AppModel
        private set

    override fun onCreate() {
        super.onCreate()
        AndroidCore.init(this, filesDir.absolutePath)
        model = AppModel(this)
    }
}
