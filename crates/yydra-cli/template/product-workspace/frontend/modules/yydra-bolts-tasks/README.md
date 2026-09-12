<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

# Pinned third-party Bolts Tasks source

`vendor/` is an unmodified source import from BoltsFramework/Bolts-Android commit
`5465bcc3bbea3350dbb2affb4511a5726efb321e`: all 13 Java production files under
`bolts-tasks/src/main/java` and the repository's complete MIT `LICENSE`.
These bytes are not Yydra-authored and are not relicensed under Yydra's dual terms.
Do not edit the import; a different source tree requires a new Distribution review.

The authored config plugin replaces requests for exactly
`com.parse.bolts:bolts-tasks:1.4.0` with this newly compiled Android library.
The old Maven JAR is neither reused nor relabeled. The later upstream source also
fixes cancellation-registration cleanup; it is not asserted byte-equivalent to 1.4.0.
Generated native projects remain disposable; Gradle outputs go under their build tree.

The Distribution retains the original LICENSE and file-level attribution record.
`yydra build --target android` performs normal Android generation and release
assembly. Doctor diagnoses Workspace identity and tools; it does not audit this
third-party source. A successful build does not prove Android runtime behavior
or blanket legal compatibility.
