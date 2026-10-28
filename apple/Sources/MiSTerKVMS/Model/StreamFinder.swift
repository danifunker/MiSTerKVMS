import Foundation
import Network

/// Capture server streams on the network (`_mkvm._tcp`, one entry per
/// stream), found through the system's Bonjour (Network.framework) rather
/// than the core's own mDNS socket: a sandboxed app then needs only the
/// network client entitlement, not the server one (apple/AppStore/README.md).
/// As on the iPhone (ios/MiSTerKVMS/Model/StreamBrowser.swift), it keeps
/// browsing while it's shown and only the stream picked is resolved: with
/// the browser running, the system has the server's records at hand.
@MainActor
final class StreamFinder: ObservableObject {
    struct Found: Identifiable, Hashable, Sendable {
        let name: String
        let mister: String
        let server: String
        /// Every announcement of the stream: a server answers on each of its
        /// network adapters, and stale copies ("MiSTer-SS1 (2)") that never
        /// resolve can linger. The one named as the stream comes first.
        let endpoints: [NWEndpoint]
        var id: String { "\(server)/\(name)" }
    }

    @Published private(set) var found: [Found] = []
    /// The first couple of seconds of a search, before "none found" means it.
    @Published private(set) var searching = false
    private var browser: NWBrowser?
    private var searchID = 0

    /// Starts browsing, or starts over.
    func start() {
        stop()
        found = []
        searching = true
        searchID += 1
        let id = searchID
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_mkvm._tcp", domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            // One row per stream, with all its announcements.
            var byID: [String: Found] = [:]
            for result in results {
                guard case let .service(instance, _, _, _) = result.endpoint else { continue }
                var txt: [String: String] = [:]
                if case let .bonjour(record) = result.metadata { txt = record.dictionary }
                let name = txt["stream"] ?? instance
                let stream = Found(name: name, mister: txt["mister"] ?? "", server: txt["server"] ?? "", endpoints: [result.endpoint])
                guard let known = byID[stream.id] else {
                    byID[stream.id] = stream
                    continue
                }
                let endpoints = instance == name ? [result.endpoint] + known.endpoints : known.endpoints + [result.endpoint]
                byID[stream.id] = Found(name: known.name, mister: known.mister, server: known.server, endpoints: endpoints)
            }
            let unique = byID.values.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            Task { @MainActor in self?.found = unique }
        }
        browser.start(queue: .main)
        self.browser = browser
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) { [weak self] in
            guard let self, self.searchID == id else { return }
            self.searching = false
        }
    }

    func stop() {
        browser?.cancel()
        browser = nil
        searching = false
    }

    /// The stream's `mkvm://` address, with the IPv4 address this Mac
    /// reaches it on: all its announcements are tried at once, the first to
    /// answer wins. Nil if none answers within a few seconds.
    nonisolated static func address(of stream: Found) async -> String? {
        await withTaskGroup(of: String?.self) { group in
            for endpoint in stream.endpoints {
                group.addTask { await connect(to: endpoint, stream: stream.name, timeout: 8) }
            }
            for await address in group {
                if let address {
                    group.cancelAll()
                    return address
                }
            }
            return nil
        }
    }

    private nonisolated static func connect(to endpoint: NWEndpoint, stream: String, timeout: TimeInterval) async -> String? {
        let params = NWParameters.tcp
        if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
            ip.version = .v4
        }
        let connection = NWConnection(to: endpoint, using: params)
        let queue = DispatchQueue(label: "StreamFinder.resolve")
        // Cancelled once another announcement answered.
        return await withTaskCancellationHandler { await withCheckedContinuation { continuation in
            let once = Once(continuation)
            let finish: @Sendable (String?) -> Void = { value in
                connection.cancel()
                once.resume(value)
            }
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    var result: String?
                    if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint {
                        var text = "\(host)"
                        if let scope = text.firstIndex(of: "%") { text = String(text[..<scope]) }
                        let name = stream.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? stream
                        result = port.rawValue == 5970 ? "mkvm://\(text)/\(name)" : "mkvm://\(text):\(port.rawValue)/\(name)"
                    }
                    finish(result)
                case .failed, .cancelled:
                    finish(nil)
                default:
                    break
                }
            }
            connection.start(queue: queue)
            queue.asyncAfter(deadline: .now() + timeout) { finish(nil) }
        } } onCancel: {
            connection.cancel()
        }
    }
}

/// Resumes a continuation at most once (connection callbacks and the timeout race).
private final class Once: @unchecked Sendable {
    private var continuation: CheckedContinuation<String?, Never>?
    private let lock = NSLock()

    init(_ continuation: CheckedContinuation<String?, Never>) { self.continuation = continuation }

    func resume(_ value: String?) {
        lock.lock()
        let c = continuation
        continuation = nil
        lock.unlock()
        c?.resume(returning: value)
    }
}
