// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "AltimCore",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "AltimCore", targets: ["AltimCore"]),
    ],
    dependencies: [
        // Only used on Linux (CI); Apple platforms use CryptoKit.
        .package(url: "https://github.com/apple/swift-crypto.git", from: "3.0.0"),
    ],
    targets: [
        // System SQLite for Linux (CI); iOS/macOS use the SDK's SQLite3 module.
        .systemLibrary(
            name: "CSQLite",
            path: "Sources/CSQLite",
            pkgConfig: "sqlite3",
            providers: [.apt(["libsqlite3-dev"])]
        ),
        .target(
            name: "AltimCore",
            dependencies: [
                .product(name: "Crypto", package: "swift-crypto", condition: .when(platforms: [.linux])),
                .target(name: "CSQLite", condition: .when(platforms: [.linux])),
            ]
        ),
        .testTarget(name: "AltimCoreTests", dependencies: ["AltimCore"]),
        // Compiled WITHOUT @testable: guarantees that everything the iOS app uses is really public.
        .testTarget(name: "PublicAPITests", dependencies: ["AltimCore"]),
    ]
)
