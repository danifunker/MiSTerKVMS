import Foundation
import Network
#if canImport(KVMSCore)
import KVMSCore
#endif

/// Capture server streams on the network (`_mkvm._tcp`, one entry per
/// stream), found through the system's Bonjour (Network.framework) rather
/// than the core's own mDNS socket: a sandboxed app then needs only the
/// network client entitlement, not the server one (apple/AppStore/README.md).
enum StreamFinder {
    fileprivate struct Found {
        let name: String
        let mister: String
        let server: String
        let endpoint: NWEndpoint
    }

    /// Browses for `wait` seconds, then resolves each stream to the IPv4
    /// address this Mac reaches it on. Streams that can't be reached are left out.
    static func find(wait: TimeInterval) async -> [FoundStream] {
        let found = await browse(wait: wait)
        let streams = await withTaskGroup(of: FoundStream?.self) { group in
            for stream in found {
                group.addTask {
                    guard let address = await address(of: stream) else { return nil }
                    return FoundStream(name: stream.name, address: address, mister: stream.mister, server: stream.server)
                }
            }
            var all: [FoundStream] = []
            for await stream in group {
                if let stream { all.append(stream) }
            }
            return all
        }
        return streams.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    private static func browse(wait: TimeInterval) async -> [Found] {
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_mkvm._tcp", domain: nil), using: .tcp)
        let queue = DispatchQueue(label: "StreamFinder")
        let latest = Latest()
        return await withCheckedContinuation { continuation in
            browser.browseResultsChangedHandler = { results, _ in
                latest.list = results.compactMap { result in
                    guard case let .service(name, _, _, _) = result.endpoint else { return nil }
                    var txt: [String: String] = [:]
                    if case let .bonjour(record) = result.metadata { txt = record.dictionary }
                    return Found(name: txt["stream"] ?? name, mister: txt["mister"] ?? "",
                                 server: txt["server"] ?? "", endpoint: result.endpoint)
                }
            }
            browser.start(queue: queue)
            queue.asyncAfter(deadline: .now() + wait) {
                browser.cancel()
                // A server answers on each of its network adapters: one entry per stream.
                var seen = Set<String>()
                continuation.resume(returning: latest.list.filter { seen.insert("\($0.server)/\($0.name)").inserted })
            }
        }
    }

    /// The stream's `mkvm://` address, with the IPv4 address this Mac
    /// reaches it on. Nil if it can't be reached within a few seconds.
    private static func address(of stream: Found) async -> String? {
        let params = NWParameters.tcp
        if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
            ip.version = .v4
        }
        let connection = NWConnection(to: stream.endpoint, using: params)
        let queue = DispatchQueue(label: "StreamFinder.resolve")
        return await withCheckedContinuation { continuation in
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
                        let name = stream.name.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? stream.name
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
            queue.asyncAfter(deadline: .now() + 3) { finish(nil) }
        }
    }
}

/// What the browser last saw; only touched on the browse queue.
private final class Latest: @unchecked Sendable {
    var list: [StreamFinder.Found] = []
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
