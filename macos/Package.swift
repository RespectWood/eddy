// swift-tools-version:5.9
// The macOS menu bar app. Build the Rust library first (scripts/build-rust.sh),
// then use scripts/build-app.sh to produce openstream.app.

import Foundation
import PackageDescription

let rustLibDir = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("Generated/lib")
    .path

let package = Package(
    name: "Openstream",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "openstream_ffiFFI", path: "Sources/openstream_ffiFFI"),
        .executableTarget(
            name: "Openstream",
            dependencies: ["openstream_ffiFFI"],
            path: "Sources/Openstream",
            linkerSettings: [.unsafeFlags(["-L", rustLibDir])]
        ),
    ]
)
