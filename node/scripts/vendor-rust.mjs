import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(packageRoot, "..");
const vendorRoot = resolve(packageRoot, "vendor");

const crates = ["dongler-core", "dongler-node"];
const rootFiles = ["LICENSE", "NOTICE"];

rmSync(vendorRoot, { force: true, recursive: true });
mkdirSync(vendorRoot, { recursive: true });

for (const crate of crates) {
  const source = resolve(repoRoot, "crates", crate);
  const destination = resolve(vendorRoot, crate);

  if (!existsSync(source)) {
    throw new Error(`Cannot vendor missing crate: ${source}`);
  }

  cpSync(source, destination, {
    recursive: true,
    filter: (path) => !path.includes("/target/"),
  });
}

for (const file of rootFiles) {
  cpSync(resolve(repoRoot, file), resolve(packageRoot, file));
}
