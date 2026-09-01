# Prototype evidence

This directory contains small, reviewable outputs from the runnable reference.
Large logs, dependency caches, build directories, and native artifacts remain
untracked evidence/build output.

- `h5-reading-queue-completed.png` is emitted by the Playwright scenario after
  a real PostgreSQL-backed entry is created and completed.
- `live-api-smoke.json` is emitted by `scripts/verify-live-api.sh` after ten
  live health, transition, Problem, filter, cursor, and CORS assertions pass.
- `cng-android-files.sha256` and `cng-android-file-modes.txt` are the 36-file
  inventory from one of two clean Android CNG runs. Both inventories were
  byte-, path-, and mode-identical, and the authored frontend inventory was
  unchanged before and after generation. The initial run failed because the
  Product ID contained Android-invalid hyphens; the CLI now renders a separate
  deterministic native identifier.
- `cng-ios-files.sha256` and `cng-ios-file-modes.txt` are the 17-file inventory
  from one of two clean iOS CNG runs. They are byte-, path-, and mode-identical.
  CNG generation is not counted as an iOS build or runtime result.
- `android-reading-queue-completed.png` records the API 36 emulator after the
  release APK created `AndroidNative1788237900` and completed it. The same
  session then reopened the entry and observed `Queued` plus a legal `Complete`
  action.

## Platform result

- Android CNG: pass, two clean 36-file inventories identical.
- Android release build: pass with Expo SDK 57, compile/target SDK 36, min SDK
  24, Gradle 9.3.1, Kotlin 2.1.20, and NDK 27.1.12297006. The final APK is
  96,657,889 bytes with SHA-256
  `8c81cd7245621203006f1e47a8a71e345a532e0c108f2995c59f09e0ec2d8924`.
- Android runtime: pass on emulator 36.6.11.0. Cold launch, real PostgreSQL/API
  load, create, complete, and reopen were observed. The reference deliberately
  enables Android cleartext traffic through Expo's CNG build-property plugin
  because its local backend is `http://10.0.2.2:4000`; the blank template does
  not inherit that relaxation.
- iOS CNG: pass, two clean 17-file inventories identical.
- iOS build/runtime: not run. This Linux/WSL2 host has no `xcodebuild`; no build
  or runtime claim is made.

The Android build emitted upstream Expo/React Native deprecation warnings, an
SDK XML tool-version warning, and Gradle 10 future-compatibility warnings. None
blocked the pinned build, but they remain candidate-stack maintenance evidence.
