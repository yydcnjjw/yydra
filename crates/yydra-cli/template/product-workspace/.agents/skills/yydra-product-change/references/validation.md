<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Supported validation

Use the cheapest discriminating check first, then widen only after it passes.

1. Run the new or changed unit test at its owning Rust or frontend surface.
2. Run the affected integration or Public API contract test.
3. Run `yydra generate api .` when the Public API or Generated Client is
   involved.
4. Use `yydra --message-format=json check . --node <stable-id>` to diagnose the
   smallest affected Mechanical Quality node.
5. Finish with the supported `yydra check .` entrypoint and retain its structured
   result and evidence. Native package-manager, Cargo, npm, Expo, and Gradle
   commands are useful diagnostics, but they are not a substitute for the final
   Yydra quality path.

A focused node records an incomplete run. Do not present it as aggregate or
complete conformance. A local full pass does not prove native runtime,
physical-device behavior, native accessibility, macOS/iOS behavior, Agent
performance, Agent Eval success, or Baseline Skill effect.

Do not weaken a required node, edit an exception authority, change a fixture to
fit the implementation, or hand-edit generated or exact-Distribution snapshot
bytes to make validation pass.
