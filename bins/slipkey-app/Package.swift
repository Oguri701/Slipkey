// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "SlipkeyApp",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "Slipkey", targets: ["SlipkeyApp"])
    ],
    targets: [
        .executableTarget(name: "SlipkeyApp"),
        .testTarget(
            name: "SlipkeyAppTests",
            dependencies: ["SlipkeyApp"]
        )
    ]
)
