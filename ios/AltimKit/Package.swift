// swift-tools-version:5.9
// Altim's core for the native app: server API models, HTTP client, live price stream, French formatting.
// Pure Swift (no UIKit): tested on Linux and macOS by the CI.
import PackageDescription

let package = Package(
    name: "AltimKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "AltimKit", targets: ["AltimKit"])],
    targets: [
        .target(name: "AltimKit"),
        .testTarget(name: "AltimKitTests", dependencies: ["AltimKit"], resources: [.copy("Fixtures")]),
    ]
)
