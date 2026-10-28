import Foundation
import Network

/// Capture server streams on the network (`_mkvm._tcp`, one entry per
/// stream). Browsing with Network.framework needs no multicast entitlement.
@MainActor
final class StreamBrowser: ObservableObject {
    struct Found: Identifiable, Hashable {
        let name: String
        let mister: String
        let server: String
        /// Every entry advertised for this stream ("MiSTer-SS1", "MiSTer-SS1 (2)", …);
        /// some may not resolve, so they're tried in turn.
        var endpoints: [NWEndpoint]
        var id: String { "\(server)/\(name)" }
    }

    @Published private(set) var found: [Found] = []
    private var browser: NWBrowser?

    func start() {
        guard browser == nil else { return }
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_mkvm._tcp", domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            let list = results.compactMap { result -> (instance: String, found: Found)? in
                guard case let .service(name, _, _, _) = result.endpoint else { return nil }
                var txt: [String: String] = [:]
                if case let .bonjour(record) = result.metadata { txt = record.dictionary }
                return (name, Found(name: txt["stream"] ?? name, mister: txt["mister"] ?? "",
                                    server: txt["server"] ?? "", endpoints: [result.endpoint]))
            }
            // A server answers on each of its network adapters: one row per
            // stream, keeping every entry (the plain name first).
            var byID: [String: Found] = [:]
            for (_, found) in list.sorted(by: { $0.instance < $1.instance }) {
                if byID[found.id] == nil {
                    byID[found.id] = found
                } else {
                    byID[found.id]?.endpoints += found.endpoints
                }
            }
            let unique = byID.values
                .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            Task { @MainActor in self?.found = unique }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    func stop() {
        browser?.cancel()
        browser = nil
    }

    /// The stream's `mkvm://` address, with the IPv4 address the phone
    /// reaches it on. Nil if it can't be reached within a few seconds. Each
    /// advertised entry is tried in turn, sharing the time.
    static func address(of stream: Found) async -> String? {
        let timeout = 5.0 / Double(max(stream.endpoints.count, 1))
        for endpoint in stream.endpoints {
            if let address = await address(of: stream, at: endpoint, timeout: max(timeout, 2)) { return address }
        }
        return nil
    }

    private static func address(of stream: Found, at endpoint: NWEndpoint, timeout: Double) async -> String? {
        let params = NWParameters.tcp
        if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
            ip.version = .v4
        }
        let connection = NWConnection(to: endpoint, using: params)
        return await withCheckedContinuation { continuation in
            let once = Once(continuation)
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    var result: String?
                    if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint {
                        var text = "\(host)"
                        if let scope = text.firstIndex(of: "%") { text = String(text[..<scope]) }
                        let name = stream.name.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? stream.name
                        result = port.rawValue == 5970 ? "mkvm://\(text)/\(name)" : "mkvm://\(text):\(port.rawValue)/\(name)"
                    }
                    connection.cancel()
                    once.resume(result)
                case .failed, .cancelled:
                    once.resume(nil)
                default:
                    break
                }
            }
            connection.start(queue: .main)
            DispatchQueue.main.asyncAfter(deadline: .now() + timeout) {
                connection.cancel()
                once.resume(nil)
            }
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
