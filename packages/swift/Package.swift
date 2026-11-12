// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DusrelImage",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
    ],
    products: [
        .library(name: "DusrelImage", targets: ["DusrelImage"]),
    ],
    targets: [
        // The generated C header + modulemap for the UniFFI cdylib.
        // This is a system-library target: it builds nothing itself,
        // it only exposes the `dusrelimageFFI` Clang module (the
        // header in this directory) to the Swift target below. The
        // actual library (`libdusrelimage_uniffi.dylib` / `.so`) is
        // built from `crates/dusrelimage-uniffi` and is never
        // committed — see README.md ("Linking the native library").
        .systemLibrary(
            name: "dusrelimageFFI",
            path: "Sources/dusrelimageFFI"
        ),
        // Generated UniFFI Swift bindings (`dusrelimage.swift`) plus
        // the thin idiomatic wrapper (`DusrelImageWrapper.swift`).
        .target(
            name: "DusrelImage",
            dependencies: ["dusrelimageFFI"],
            path: "Sources/DusrelImage",
            linkerSettings: [
                // Link against the cdylib built from
                // `crates/dusrelimage-uniffi`. The library search path
                // is supplied at build time (CI passes
                // `-Xlinker -L<repo>/target/release`; locally export
                // LIBRARY_PATH — see README.md). The library itself is
                // never committed.
                .linkedLibrary("dusrelimage_uniffi"),
            ]
        ),
        .testTarget(
            name: "DusrelImageTests",
            dependencies: ["DusrelImage"],
            path: "Tests/DusrelImageTests"
        ),
    ]
)
