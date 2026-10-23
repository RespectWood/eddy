// swift-tools-version:5.9
// The macOS menu bar app. Build the Rust library first (scripts/build-rust.sh),
// then use scripts/build-app.sh to produce Eddy.app.

import Foundation
import PackageDescription

let rustLibDir = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("Generated/lib")
    .path

let package = Package(
    name: "Eddy",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "eddy_ffiFFI", path: "Sources/eddy_ffiFFI"),
        .executableTarget(
            name: "Eddy",
            dependencies: ["eddy_ffiFFI"],
            path: "Sources/Eddy",
            linkerSettings: [.unsafeFlags(["-L", rustLibDir])]
        ),
    ]
)
