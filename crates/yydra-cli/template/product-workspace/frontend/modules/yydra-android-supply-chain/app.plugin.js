// SPDX-License-Identifier: MIT OR Apache-2.0
/* global require: readonly, module: readonly */

// Expo loads local config plugins through CommonJS.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const configPlugins = require("@expo/config-plugins");
const { withAppBuildGradle, withProjectBuildGradle, withSettingsGradle } =
  configPlugins;

const BEGIN_MARKER = "// yydra-android-supply-chain-constraints:begin";
const END_MARKER = "// yydra-android-supply-chain-constraints:end";

const CONSTRAINTS = `${BEGIN_MARKER}
dependencies {
    constraints {
        implementation("com.google.code.gson:gson") {
            version { strictly("2.14.0") }
            because("2.8.6 is affected by GHSA-4jrv-ppp4-jm57")
        }
        implementation("commons-io:commons-io") {
            version { strictly("2.22.0") }
            because("1.4 is affected by GHSA-gwrp-pvrq-jmwv")
        }
    }
}
${END_MARKER}
`;

function addAndroidSupplyChainConstraints(contents) {
  if (contents.includes(BEGIN_MARKER) || contents.includes(END_MARKER)) {
    if (!(contents.includes(BEGIN_MARKER) && contents.includes(END_MARKER))) {
      throw new Error(
        "incomplete Yydra Android supply-chain constraint markers",
      );
    }
    return contents;
  }

  return `${contents.trimEnd()}\n\n${CONSTRAINTS}`;
}

function withAndroidSupplyChain(config) {
  config = withSettingsGradle(config, (configured) => {
    configured.modResults.contents = addBoltsProject(
      configured.modResults.contents,
    );
    return configured;
  });
  config = withProjectBuildGradle(config, (configured) => {
    configured.modResults.contents = addBoltsSubstitution(
      configured.modResults.contents,
    );
    return configured;
  });
  return withAppBuildGradle(config, (configured) => {
    if (configured.modResults.language !== "groovy") {
      throw new Error(
        "Yydra Android supply-chain constraints require Groovy Gradle output",
      );
    }
    configured.modResults.contents = addAndroidSupplyChainConstraints(
      configured.modResults.contents,
    );
    return configured;
  });
}

module.exports = withAndroidSupplyChain;
module.exports.addAndroidSupplyChainConstraints =
  addAndroidSupplyChainConstraints;

function pinnedBlock(contents, name, body) {
  const begin = `// ${name}:begin`;
  const end = `// ${name}:end`;
  const block = `${begin}\n${body}\n${end}`;
  if (contents.includes(begin) || contents.includes(end)) {
    if (
      !contents.includes(block) ||
      contents.split(begin).length !== 2 ||
      contents.split(end).length !== 2
    ) {
      throw new Error(`incomplete or changed ${name} markers`);
    }
    return contents;
  }
  return `${contents.trimEnd()}\n\n${block}\n`;
}

function addBoltsProject(contents) {
  return pinnedBlock(
    contents,
    "yydra-bolts-project",
    `include ':yydra-bolts-tasks'
project(':yydra-bolts-tasks').projectDir = new File(rootProject.projectDir, '../modules/yydra-bolts-tasks/android')`,
  );
}

function addBoltsSubstitution(contents) {
  return pinnedBlock(
    contents,
    "yydra-bolts-substitution",
    `allprojects {
    configurations.configureEach {
        resolutionStrategy.dependencySubstitution {
            all { substitution ->
                def requested = substitution.requested
                if (requested instanceof org.gradle.api.artifacts.component.ModuleComponentSelector && requested.group == 'com.parse.bolts' && requested.module == 'bolts-tasks') {
                    if (requested.version != '1.4.0') {
                        throw new GradleException('Yydra Bolts replacement requires re-review for a changed requested version')
                    }
                    substitution.useTarget(project(':yydra-bolts-tasks'), 'Build exact MIT source 5465bcc3bbea3350dbb2affb4511a5726efb321e; never reuse the BSD+PATENTS 1.4.0 JAR')
                }
            }
        }
    }
}`,
  );
}

module.exports.addBoltsProject = addBoltsProject;
module.exports.addBoltsSubstitution = addBoltsSubstitution;
