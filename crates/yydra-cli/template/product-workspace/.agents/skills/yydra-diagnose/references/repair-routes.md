<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Safe repair routes

Route by the exact reported code and remediation; shared prefixes are not
default routes. Inspect the actual command output rather than guessing.

- `DOCTOR_WORKSPACE_VERIFY`: install the exact CLI named by the Origin Record,
  or restore reviewed Origin, inventory, provenance, license, or build-support
  snapshots. Do not edit those authorities into agreement.
- `DOCTOR_RUST_TOOL`: install nightly with rustfmt and Clippy; correct PATH or
  an incompatible toolchain override. Update nightly only explicitly.
- `DOCTOR_FRONTEND_TOOL`: install working Node/npm compatible with the project's
  dependencies. Do not change Public API source to disguise a missing or wrong tool.
- `DOCTOR_FRONTEND_DEPENDENCIES`: run `moon run product:setup` when ready to install; doctor
  is allowed to precede setup.
- `DOCTOR_OPTIONAL_DOCKER`: start/install Docker and Compose when using local
  containers, or configure an external PostgreSQL database.
- `DOCTOR_ANDROID_JDK`, `DOCTOR_ANDROID_SDK`, `DOCTOR_ANDROID_PLATFORM_TOOLS`, and
  `DOCTOR_ANDROID_COMPONENT`: repair the named installed tool or SDK path.
  Use explicit ANDROID_HOME because Android builds isolate HOME. The real build
  selects the component versions needed by the locked Expo/React Native inputs.
- `DOCTOR_CANCELLED`: the user cancelled diagnosis; no successful result exists.
- `SETUP_NPM_CI`: inspect the retained npm log first and repair the reported
  configuration, tool, or infrastructure condition. Preserve locked URLs, versions, and integrities;
  do not regenerate the lock unless dependency drift or an explicitly intended dependency change
  is established.
- `NATIVE_GENERATION_CLEANUP_FAILED`: resolve the reported filesystem or process
  cleanup condition. Do not edit authored Expo inputs for a cleanup failure.
- `NATIVE_GENERATION_MUTATED_AUTHORED_INPUTS`: repair the authored generator inputs
  that changed during generation. Never patch generated native source.
- `NATIVE_GENERATION_FAILED`, `NATIVE_GENERATION_OUTPUT_MISSING`, and
  `ANDROID_RELEASE_BUILD_FAILED`: inspect `frontend/.expo/yydra-build/android.log`
  and repair the identified authored input or missing tool.
- `ANDROID_RELEASE_OUTPUT_MISSING`: inspect the Gradle failure/output path;
  never fabricate an APK or report a build without its actual artifact.

For database and H5 tests, prepare an isolated PostgreSQL database and a migrated
backend explicitly. Do not edit Product Presentation to disguise availability
or a migration failure. Unlisted codes require their concrete command output
and remediation before choosing a repair.
