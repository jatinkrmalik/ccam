// Downloads the ccam binary that matches this package's version from the GitHub
// release, verifies its SHA-256 and unpacks it into ./vendor.
"use strict";

const crypto = require("crypto");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const REPO = "jatinkrmalik/ccam";
const version = "v" + require("./package.json").version;
const vendor = path.join(__dirname, "vendor");
const exe = process.platform === "win32" ? "ccam.exe" : "ccam";
const binary = path.join(vendor, exe);

function target() {
  const key = process.platform + "-" + process.arch;
  const targets = {
    "darwin-arm64": "aarch64-apple-darwin",
    "darwin-x64": "x86_64-apple-darwin",
    "linux-x64": "x86_64-unknown-linux-musl",
    "win32-x64": "x86_64-pc-windows-msvc",
    "win32-arm64": "x86_64-pc-windows-msvc",
  };
  if (!targets[key]) {
    throw new Error("no prebuilt ccam binary for " + key + "; install with: cargo install ccam");
  }
  return targets[key];
}

function download(url, redirects) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "ccam-npm-installer" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          res.resume();
          if (redirects > 5) return reject(new Error("too many redirects"));
          return resolve(download(res.headers.location, redirects + 1));
        }
        if (res.statusCode !== 200) {
          res.resume();
          return reject(new Error("HTTP " + res.statusCode + " for " + url));
        }
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

async function install() {
  if (fs.existsSync(binary)) return binary;
  const name = "ccam-" + version + "-" + target();
  const archive = name + (process.platform === "win32" ? ".zip" : ".tar.gz");
  const base = "https://github.com/" + REPO + "/releases/download/" + version + "/";

  const [data, sums] = await Promise.all([download(base + archive, 0), download(base + archive + ".sha256", 0)]);
  const expected = sums.toString("utf8").trim().split(/\s+/)[0].toLowerCase();
  const actual = crypto.createHash("sha256").update(data).digest("hex");
  if (actual !== expected) throw new Error("checksum did not match for " + archive);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ccam-"));
  try {
    const file = path.join(tmp, archive);
    fs.writeFileSync(file, data);
    // bsdtar ships with Windows 10+ and macOS and unpacks .zip as well as .tar.gz.
    const tar = process.platform === "win32" ? path.join(process.env.SystemRoot || "C:\Windows", "System32", "tar.exe") : "tar";
    execFileSync(tar, ["-xf", file, "-C", tmp], { stdio: "ignore" });
    fs.mkdirSync(vendor, { recursive: true });
    fs.copyFileSync(path.join(tmp, name, exe), binary);
    if (process.platform !== "win32") fs.chmodSync(binary, 0o755);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  return binary;
}

module.exports = { install, binary };

if (require.main === module) {
  install().catch((err) => {
    // Do not fail `npm install`; bin/ccam.js retries on first run and reports the error then.
    console.warn("ccam: could not download the binary yet (" + err.message + "). It will be retried on first run.");
  });
}
