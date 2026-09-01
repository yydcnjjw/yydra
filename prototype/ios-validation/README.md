# Yydra V0 iOS validation prototype

This throwaway prototype answers one question: can the pinned React Native/Expo
candidate build reproducibly and run the Reading Queue Reference Product against
its real PostgreSQL-backed API on a supported macOS/Xcode grader, while keeping
the generated `ios/` host disposable?

The GitHub Actions run records three separate evidence tiers:

1. Two clean Expo CNG runs with path, byte, size, and mode inventories plus an
   authored-source status comparison.
2. The Distribution-owned `macos-ci` quality shard, which builds a Release
   simulator `.app`, records its digest, preserves an executable archive across
   artifact transport, removes generated native/build trees, and contributes to
   the fail-closed aggregate result.
3. A fresh iPhone 17 / iOS 26.5 simulator run. Maestro drives list, create,
   complete, and reopen through the rendered UI; a database query verifies the
   final Product Domain state in PostgreSQL 18.6.

The Reference Product declares `NSAllowsLocalNetworking` through Expo's
Product-owned `ios.infoPlist` input so its local HTTP API is explicit. The
prototype never hand-edits or commits the generated `ios/` tree. No physical
device claim is made.

Harness-only retries use the `V0 iOS Runtime Continuation` workflow. It consumes
one explicitly identified `quality-reference-macos` artifact from the full
quality run instead of rebuilding the application, and the runtime result
records the build run, build commit, and runtime-harness commit separately.

Pinned external grader inputs:

- GitHub `macos-26` runner image and Xcode 26.6
- CocoaPods 1.17.0
- Node.js 26.8.1
- PostgreSQL 18.6 from Homebrew `postgresql@18`
- Maestro CLI 2.7.0, release asset SHA-256
  `a4ccab6b604617e7aef6db4f885666056eabe5cfa32befaa3bc994041b8fcbb5`

Primary references:

- <https://github.com/actions/runner-images/blob/main/images/macos/macos-26-Readme.md>
- <https://formulae.brew.sh/formula/postgresql@18>
- <https://github.com/mobile-dev-inc/Maestro/releases/tag/cli-2.7.0>
- <https://docs.maestro.dev/cli/start-device>
- <https://docs.expo.dev/versions/latest/config/app/#infoplist>
- <https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking>
