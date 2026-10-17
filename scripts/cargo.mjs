import { existsSync } from "node:fs";
import { delimiter, join } from "node:path";
import { spawn } from "node:child_process";

const root = new URL("../", import.meta.url).pathname.replace(/^\/(\w:)/, "$1");
const localCargoHome = join(root, ".tools", "cargo");
const localRustupHome = join(root, ".tools", "rust");
const localCargo = join(localCargoHome, "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
const command = existsSync(localCargo) ? localCargo : "cargo";
const env = { ...process.env };

if (existsSync(localCargo)) {
  env.CARGO_HOME = localCargoHome;
  env.RUSTUP_HOME = localRustupHome;
  env.PATH = `${join(localCargoHome, "bin")}${delimiter}${env.PATH ?? ""}`;
}

if (process.platform === "win32") {
  const llvmRoot = join(root, ".tools", "llvm-mingw-20260908-ucrt-x86_64");
  const llvmBin = join(llvmRoot, "bin");
  const linker = join(llvmBin, "x86_64-w64-mingw32-clang.exe");
  const compatibility = join(root, ".tools", "gnu-compat");
  if (existsSync(linker) && existsSync(compatibility)) {
    env.RUSTUP_TOOLCHAIN ??= "stable-x86_64-pc-windows-gnu";
    env.PATH = `${llvmBin}${delimiter}${env.PATH ?? ""}`;
    env.CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = linker;
    env.RUSTFLAGS = `${env.RUSTFLAGS ?? ""} -Lnative=${compatibility}`.trim();
    env.CARGO_BUILD_JOBS ??= "1";
  }
}

const child = spawn(command, process.argv.slice(2), { cwd: root, env, stdio: "inherit" });
child.on("exit", (code) => { process.exitCode = code ?? 1; });
child.on("error", (error) => { console.error(error.message); process.exitCode = 1; });
