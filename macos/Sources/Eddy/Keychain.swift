import Foundation
import Security

/// Generic-password items in the login keychain, for account credentials.
enum Keychain {
    /// Readable once the Mac has been unlocked (Eddy works in the background), and
    /// never restored onto another Mac from a backup.
    private static let accessible = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

    static func read(service: String, account: String) -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }

    @discardableResult
    static func write(_ data: Data, service: String, account: String) -> Bool {
        let match: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        // Setting `accessible` on update too moves items saved by older builds over.
        let update: [String: Any] = [kSecValueData as String: data, kSecAttrAccessible as String: accessible]
        if SecItemUpdate(match as CFDictionary, update as CFDictionary) == errSecSuccess { return true }
        var add = match
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = accessible
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    static func readString(service: String, account: String) -> String? {
        read(service: service, account: account).flatMap { String(data: $0, encoding: .utf8) }
    }

    static func delete(service: String, account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
