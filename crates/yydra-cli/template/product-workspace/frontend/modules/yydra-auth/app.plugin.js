// SPDX-License-Identifier: MIT OR Apache-2.0
/* global require: readonly, module: readonly */
// eslint-disable-next-line @typescript-eslint/no-require-imports
const configPlugins = require("@expo/config-plugins");
const { AndroidConfig, withAndroidManifest, withDangerousMod } = configPlugins;
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { copyFile, mkdir } = require("node:fs/promises");
// eslint-disable-next-line @typescript-eslint/no-require-imports
const path = require("node:path");

module.exports = function withAuthNetwork(config) {
  config = withAndroidManifest(config, (configured) => {
    const application = AndroidConfig.Manifest.getMainApplicationOrThrow(
      configured.modResults,
    );
    const resource = "@xml/yydra_auth_network_security";
    const existing = application.$["android:networkSecurityConfig"];
    if (existing && existing !== resource) {
      throw new Error(
        "Merge the product network security policy with the authentication loopback policy explicitly",
      );
    }
    application.$["android:networkSecurityConfig"] = resource;
    return configured;
  });
  return withDangerousMod(config, [
    "android",
    async (configured) => {
      for (const variant of ["main", "debug"]) {
        const directory = path.join(
          configured.modRequest.platformProjectRoot,
          "app/src",
          variant,
          "res/xml",
        );
        await mkdir(directory, { recursive: true });
        await copyFile(
          path.join(__dirname, `${variant}-network-security.xml`),
          path.join(directory, "yydra_auth_network_security.xml"),
        );
      }
      return configured;
    },
  ]);
};
