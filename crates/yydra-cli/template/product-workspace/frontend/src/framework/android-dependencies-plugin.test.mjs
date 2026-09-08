// SPDX-License-Identifier: MIT OR Apache-2.0

import { createRequire } from "node:module";

import { describe, expect, it } from "vitest";

const require = createRequire(import.meta.url);
const {
  addAndroidDependencyConstraints,
  addBoltsProject,
  addBoltsSubstitution,
} = require("../../modules/yydra-android-dependencies/app.plugin.js");

describe("Android dependency config plugin", () => {
  it("replaces only the reviewed old Bolts request with a source-built project", () => {
    const settings = addBoltsProject("");
    const root = addBoltsSubstitution("");
    expect(settings).toContain("../modules/yydra-bolts-tasks/android");
    expect(root).toContain("requested.version != '1.4.0'");
    expect(root).toContain(
      "substitution.useTarget(project(':yydra-bolts-tasks')",
    );
    expect(root).toContain("5465bcc3bbea3350dbb2affb4511a5726efb321e");
    expect(addBoltsProject(settings)).toBe(settings);
    expect(addBoltsSubstitution(root)).toBe(root);
    expect(() =>
      addBoltsSubstitution(root.replace("'1.4.0'", "'1.5.0'")),
    ).toThrow("changed");
    expect(() => addBoltsProject("// yydra-bolts-project:begin")).toThrow(
      "incomplete",
    );
  });
  it("adds exact reviewed dependency constraints once", () => {
    const initial = "dependencies {\n}\n";
    const once = addAndroidDependencyConstraints(initial);
    const twice = addAndroidDependencyConstraints(once);

    expect(once).toContain('strictly("2.14.0")');
    expect(once).toContain('strictly("2.22.0")');
    expect(twice).toBe(once);
    expect(
      once.match(/yydra-android-dependencies-constraints:begin/g),
    ).toHaveLength(1);
  });

  it("fails closed when only one generated marker remains", () => {
    expect(() =>
      addAndroidDependencyConstraints(
        "// yydra-android-dependencies-constraints:begin\n",
      ),
    ).toThrow("incomplete Yydra Android dependency constraint markers");
  });
});
