import { access, readFile, readdir, rename } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(
  await readFile(join(root, "package.json"), "utf8"),
);
const bundleDirectory = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const sourceName = `Tocky Voice Automotive_${packageJson.version}_x64-setup.exe`;

function timestamp(date) {
  const part = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}${part(date.getMonth() + 1)}${part(date.getDate())}-${part(date.getHours())}${part(date.getMinutes())}`;
}

function run(command, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
    });
    child.once("error", rejectRun);
    child.once("exit", (code) => {
      if (code === 0) {
        resolveRun();
        return;
      }
      rejectRun(new Error(`${command} exited with code ${code ?? "unknown"}`));
    });
  });
}

const pnpmCli = process.env.npm_execpath;
if (!pnpmCli) {
  throw new Error("Run this command through pnpm so its executable path is known.");
}

await run(process.execPath, [
  pnpmCli,
  "tauri",
  "build",
  "--bundles",
  "nsis",
  ...process.argv.slice(2),
]);

const source = join(bundleDirectory, sourceName);
await access(source);

const targetName = sourceName.replace(
  "_x64-setup.exe",
  `_${timestamp(new Date())}_x64-setup.exe`,
);
const target = join(bundleDirectory, targetName);

try {
  await access(target);
  throw new Error(`Refusing to overwrite existing installer: ${targetName}`);
} catch (error) {
  if (error?.code !== "ENOENT") throw error;
}

await rename(source, target);

const installers = (await readdir(bundleDirectory))
  .filter((name) => name.endsWith("_x64-setup.exe"))
  .sort();

console.log(`Timestamped Windows installer: ${target}`);
console.log(`Available Windows installers: ${installers.join(", ")}`);
