import Foundation
import Security
#if canImport(KVMSCore)
import KVMSCore
#endif

/// JetKVM passwords, in the login Keychain (one item per JetKVM address).
/// The core never stores them: it gets them in memory with
/// `setJetkvmPassword` before it opens a JetKVM (docs/JETKVM.md).
enum JetKVMPasswords {
    private static let service = "MiSTerKVMS JetKVM"

    /// The key a JetKVM is filed under: its address, as the core keys it.
    static func key(_ address: String) -> String {
        var a = address.trimmingCharacters(in: .whitespaces)
        for prefix in ["jetkvm://", "http://"] where a.hasPrefix(prefix) { a.removeFirst(prefix.count) }
        while a.hasSuffix("/") { a.removeLast() }
        return a.lowercased()
    }

    static func load(_ address: String) -> String {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key(address),
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data, let password = String(data: data, encoding: .utf8) else { return "" }
        return password
    }

    /// Save (or, empty, delete) the password and hand it to the core.
    @discardableResult
    static func save(_ password: String, for address: String) -> Bool {
        let account: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key(address),
        ]
        SecItemDelete(account as CFDictionary)
        var ok = true
        if !password.isEmpty {
            var item = account
            item[kSecValueData as String] = Data(password.utf8)
            item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            ok = SecItemAdd(item as CFDictionary, nil) == errSecSuccess
        }
        KVMSCore.setJetkvmPassword(address: key(address), password: password)
        return ok
    }

    /// At start-up and when hosts change: the core gets every JetKVM
    /// password the hosts need.
    static func hand(to profiles: [HostProfile]) {
        for address in Set(profiles.compactMap(\.jetkvmAddress)) {
            KVMSCore.setJetkvmPassword(address: key(address), password: load(address))
        }
    }
}

extension HostProfile {
    /// The JetKVM this host's video comes from, if it's one (`jetkvm://…`).
    var jetkvmAddress: String? {
        guard sourceKind == "local", localVideo.hasPrefix("jetkvm://") else { return nil }
        let a = JetKVMPasswords.key(localVideo)
        return a.isEmpty ? nil : a
    }
}
