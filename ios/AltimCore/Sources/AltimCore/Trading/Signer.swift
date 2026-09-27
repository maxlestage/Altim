import Foundation
#if canImport(CryptoKit)
import CryptoKit
#else
import Crypto
#endif

enum Signer {
    /// HMAC-SHA256 hexadécimal (signature des requêtes Binance).
    static func hmacSHA256Hex(key: String, message: String) -> String {
        let mac = HMAC<SHA256>.authenticationCode(for: Data(message.utf8), using: SymmetricKey(data: Data(key.utf8)))
        return mac.map { String(format: "%02x", $0) }.joined()
    }
}
