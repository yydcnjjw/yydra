<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Replace the selected Bolts artifact with a pinned MIT source build

Status: selected implementation direction; release acceptance pending in Issue #39.

## Scope amendment — 2026-09-05

The maintainer subsequently instructed “不考虑许可证和来源”. The current scope
amendment in Issue #26 and revised Issue #39 supersede the dependency-license
admission, notice-completeness, source-trust and upstream-provenance review gates
described below. Those paragraphs record the earlier decision, not current gates.
The existing Bolts source replacement and its LICENSE/attribution are retained;
this change does not relabel the historical Maven artifact or authorize a revert.
Exact inventories, advisory checks, lock/Distribution identity, artifact checksums,
and packaged clean/Reading Queue acceptance remain required. The implementation
now records the Distribution-declared identity without verifying local source
against that commit. Focused scope regressions are not full packaged conformance;
fresh clean/Reading Queue acceptance remains pending in Issue #39.

## Context and authority

Issue #26 requires fail-closed license/provenance checks on actual Android materials.
Fresco 3.6.0 selects `com.parse.bolts:bolts-tasks:1.4.0`. That historical
artifact corresponds to BSD plus PATENTS terms, while the repository's later
mutable LICENSE is MIT. The old artifact cannot be relabeled using that later text.
Fresco 3.7.0 still selects 1.4.0. The current Parse fork changes the Java package
and retains a PATENTS file; it is not a verified drop-in permissive replacement.
The MIT commit has no usable JitPack artifact in the checked build result.

The user authorized actual replacement/upgrade, retaining all acceptance criteria:
https://github.com/yydcnjjw/yydra/issues/39#issuecomment-5549302063.
This does not approve the old BSD+PATENTS material or authorize publication.

## Decision

Import without edits the 13 production Java files and root MIT LICENSE from
`BoltsFramework/Bolts-Android@5465bcc3bbea3350dbb2affb4511a5726efb321e`.
The Distribution embeds an exact path/mode/SHA-256 source manifest. The source
tree is third-party material, not Yydra-authored code or a copied Framework fork.
New Gradle/config-plugin glue uses `MIT OR Apache-2.0` and is verified separately.
The closed Android adapter directory cannot add default variant source inputs.

An authored Expo config plugin substitutes only requests for exactly the old
1.4.0 coordinate with the newly built local Android library. A changed requested
version requires re-review. Outputs remain in the disposable native build tree;
no generated native project is patched as source, and no binary is published.

The source component receives its own GitHub purl, complete MIT notice, source
manifest digest, and exact-commit OSV request. It is copied in the packaged CLI
template and becomes Android linked material only with the retained Gradle
project/variant/AAR evidence. It never inherits npm lock authority. The final
Android material graph must show the requested-to-selected replacement and no
old Maven Bolts component. Source changes, missing/extra inputs, missing notices,
missing producer bindings, and advisory service failures remain blocking.

## Consequences and validation boundary

Yydra now maintains one explicitly pinned third-party source import; this is not
a general-purpose local dependency allowlist or an automatic update mechanism.
The post-1.4.0 cancellation cleanup change is real and requires compatibility
testing. Source/blob checks, JVM task/cancellation diagnostics, and config-plugin
tests do not substitute for the packaged CLI/fresh consumer acceptance seam.
Clean and Reading Queue conformance, H5 behavior, deterministic CNG, the Android
release build, final SBOM/notices, and all existing acceptance conditions remain.
No Android runtime support or blanket legal-compatibility claim is added.
