import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const localManifest = resolve(packageRoot, "../crates/dongler-node/Cargo.toml");
const vendoredManifest = resolve(packageRoot, "vendor/dongler-node/Cargo.toml");
const manifestPath = existsSync(localManifest) ? localManifest : vendoredManifest;

if (!existsSync(manifestPath)) {
  throw new Error("Cannot find Dongler Rust manifest for native build.");
}

const napiBin = process.platform === "win32" ? "napi.cmd" : "napi";
const result = spawnSync(
  napiBin,
  [
    "build",
    "--release",
    "--manifest-path",
    manifestPath,
    "--output-dir",
    packageRoot,
    "--package-json-path",
    resolve(packageRoot, "package.json"),
  ],
  {
    cwd: packageRoot,
    env: process.env,
    stdio: "inherit",
  }
);

if (result.error) {
  throw result.error;
}

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
