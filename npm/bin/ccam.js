#!/usr/bin/env node
"use strict";

const fs = require("fs");
const { spawnSync } = require("child_process");
const { install, binary } = require("../install.js");

async function main() {
  if (!fs.existsSync(binary)) {
    try {
      await install();
    } catch (err) {
      console.error("ccam: could not download the ccam binary: " + err.message);
      console.error("Download it from https://github.com/jatinkrmalik/ccam/releases instead.");
      process.exit(1);
    }
  }
  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    console.error("ccam: " + result.error.message);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
}

main();
