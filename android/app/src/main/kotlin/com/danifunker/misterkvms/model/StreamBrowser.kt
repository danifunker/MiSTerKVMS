package com.danifunker.misterkvms.model

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.net.Inet4Address
import java.net.InetAddress

/** Capture server streams on the network (`_mkvm._tcp`, one entry per
 *  stream). Android gives a service's TXT record (stream, MiSTer, server)
 *  only when it's resolved, so each one found is resolved in turn: before
 *  Android 14 only one resolve may run at a time. */
class StreamBrowser(context: Context) {
    data class Found(
        val name: String,
        val mister: String,
        val server: String,
        /** `mkvm://ip[:port]/name`, with the IPv4 address the phone reaches it on. */
        val address: String,
        /** The DNS-SD service name, to drop the row when it's gone. */
        val service: String,
    ) {
        val id: String get() = "$server/$name"
    }

    var found: List<Found> by mutableStateOf(emptyList())
        private set

    private val nsd = context.getSystemService(NsdManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var listener: NsdManager.DiscoveryListener? = null
    private val waiting = ArrayDeque<NsdServiceInfo>()
    private var resolving = false

    fun start() {
        if (listener != null) return
        val l = object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                main.post { queue(info) }
            }

            override fun onServiceLost(info: NsdServiceInfo) {
                main.post { found = found.filterNot { it.service == info.serviceName } }
            }

            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onDiscoveryStopped(serviceType: String) {}
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {}
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
        }
        listener = l
        nsd.discoverServices("_mkvm._tcp", NsdManager.PROTOCOL_DNS_SD, l)
    }

    fun stop() {
        listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        listener = null
        waiting.clear()
    }

    private fun queue(info: NsdServiceInfo) {
        waiting.addLast(info)
        resolveNext()
    }

    @Suppress("DEPRECATION") // resolveService: the one way before Android 14
    private fun resolveNext() {
        if (resolving || listener == null) return
        val info = waiting.removeFirstOrNull() ?: return
        resolving = true
        nsd.resolveService(info, object : NsdManager.ResolveListener {
            override fun onServiceResolved(resolved: NsdServiceInfo) {
                main.post {
                    add(resolved)
                    resolving = false
                    resolveNext()
                }
            }

            override fun onResolveFailed(failed: NsdServiceInfo, errorCode: Int) {
                main.post {
                    resolving = false
                    resolveNext()
                }
            }
        })
    }

    private fun add(info: NsdServiceInfo) {
        val txt = info.attributes.mapValues { (_, v) -> v?.toString(Charsets.UTF_8) ?: "" }
        val host = ipv4(info) ?: return
        val name = txt["stream"]?.takeIf { it.isNotEmpty() } ?: info.serviceName
        val path = android.net.Uri.encode(name)
        val address = if (info.port == 5970) "mkvm://$host/$path" else "mkvm://$host:${info.port}/$path"
        val stream = Found(name, txt["mister"] ?: "", txt["server"] ?: "", address, info.serviceName)
        // A server answers on each of its network adapters: one row per stream.
        if (found.none { it.id == stream.id }) {
            found = (found + stream).sortedBy { it.name.lowercase() }
        }
    }

    @Suppress("DEPRECATION") // host: hostAddresses only from Android 14
    private fun ipv4(info: NsdServiceInfo): String? {
        val addresses: List<InetAddress> =
            if (Build.VERSION.SDK_INT >= 34) info.hostAddresses else listOfNotNull(info.host)
        return addresses.firstOrNull { it is Inet4Address }?.hostAddress
    }
}
