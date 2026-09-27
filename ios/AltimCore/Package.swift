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
        .target(
            name: "AltimCore",
            dependencies: [
                .product(name: "Crypto", package: "swift-crypto", condition: .when(platforms: [.linux])),
            ]
        ),
        .testTarget(name: "AltimCoreTests", dependencies: ["AltimCore"]),
    ]
)
