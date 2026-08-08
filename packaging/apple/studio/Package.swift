// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "studio",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "studio",
            path: "Sources/studio"
        )
    ]
)
