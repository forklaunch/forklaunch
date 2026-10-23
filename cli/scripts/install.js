#!/usr/bin/env node

const { execSync, execFileSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const os = require("os");
const { pipeline } = require("stream");

const GITHUB_REPO = "forklaunch/forklaunch-js";
const VERSION = require("../package.json").version;

function getPlatform() {
  const type = process.platform;
  const arch = process.arch;

  if (type === "darwin") {
    return arch === "arm64" ? "darwin-aarch64" : "darwin-x86_64";
  }
  if (type === "linux") {
    return arch === "arm64" ? "linux-aarch64" : "linux-x86_64";
  }
  if (type === "win32") {
    return "windows-x86_64";
  }

  throw new Error(`Unsupported platform: ${type} ${arch}`);
}

function semverIsNewer(newVersion, oldVersion) {
  const newParts = newVersion.split(".").map(Number);
  const oldParts = oldVersion.split(".").map(Number);

  for (let i = 0; i < Math.max(newParts.length, oldParts.length); i++) {
    const newPart = newParts[i] || 0;
    const oldPart = oldParts[i] || 0;
    if (newPart > oldPart) return true;
    if (newPart < oldPart) return false;
  }
  return false;
}

function findExecutablePath(name) {
  const command =
    process.platform === "win32" ? `where ${name}.exe` : `which ${name}`;
  try {
    const result = execSync(command, { stdio: "pipe" }).toString().trim();
    const firstPath = result.split(/\r?\n/)[0];
    if (fs.existsSync(firstPath)) {
      return firstPath;
    }
    return null;
  } catch (e) {
    return null;
  }
}

function getTargetBinDir() {
  return path.join(os.homedir(), ".forklaunch", "bin");
}

function updatePathVariable(binDir) {
  const absoluteBinDir = path.resolve(binDir);

  if (process.platform === "win32") {
    console.log("Adding install directory to your PATH...");
    const powershellCommand = `
      $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User");
      if (($currentUserPath -split ';') -notcontains "${absoluteBinDir}") {
        $newUserPath = $currentUserPath + ";${absoluteBinDir}";
        [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User");
        Write-Host "Installation directory added to your PATH.";
      } else {
        Write-Host "Installation directory is already in your PATH.";
      }
    `;
    try {
      // Pass the script as one argument; no shell quoting to get wrong.
      execFileSync("powershell", ["-NoProfile", "-Command", powershellCommand], {
        stdio: "inherit"
      });
    } catch (e) {
      console.error(
        "Failed to update PATH with PowerShell. Please add it manually."
      );
      console.warn(
        `You need to add the following directory to your User PATH: ${absoluteBinDir}`
      );
    }
  } else {
    const shell = process.env.SHELL || "";
    let shellConfigFile = null;
    const homeDir = os.homedir();
    const commandToAdd = `\n# ForkLaunch Path\nexport PATH="${absoluteBinDir}:$PATH"\n`;

    if (shell.includes("zsh")) {
      shellConfigFile = path.join(homeDir, ".zshrc");
    } else if (shell.includes("bash")) {
      shellConfigFile = path.join(homeDir, ".bash_profile");
      if (!fs.existsSync(shellConfigFile)) {
        shellConfigFile = path.join(homeDir, ".bashrc");
      }
    } else {
      console.warn(
        `Unsupported shell: ${shell}. Please add the following to your shell config file:`
      );
      console.warn(commandToAdd);
      return;
    }

    if (!fs.existsSync(shellConfigFile)) {
      fs.writeFileSync(shellConfigFile, "");
    }

    const fileContent = fs.readFileSync(shellConfigFile, "utf8");
    if (fileContent.includes(absoluteBinDir)) {
      console.log("Installation directory is already in your PATH.");
    } else {
      console.log(
        `Updating ${path.basename(shellConfigFile)} to include forklaunch...`
      );
      fs.appendFileSync(shellConfigFile, commandToAdd);
    }
  }
}

function createAlias(source, target) {
  if (fs.existsSync(target)) {
    fs.unlinkSync(target);
  }

  if (process.platform === "win32") {
    fs.copyFileSync(source, target);
  } else {
    fs.symlinkSync(source, target);
  }
}

function downloadBinary() {
  const platform = getPlatform();
  const binaryName =
    process.platform === "win32" ? "forklaunch.exe" : "forklaunch";
  const aliasName = process.platform === "win32" ? "fl.exe" : "fl";
  const artifactName =
    process.platform === "win32"
      ? `forklaunch-${platform}.exe`
      : `forklaunch-${platform}`;
  const downloadUrl = `https://github.com/${GITHUB_REPO}/releases/download/cli-v${VERSION}/${artifactName}`;

  let binDir;
  try {
    binDir = getTargetBinDir();
  } catch (e) {
    return Promise.reject(e);
  }

  const binaryPath = path.join(binDir, binaryName);
  const aliasPath = path.join(binDir, aliasName);

  if (fs.existsSync(binaryPath)) {
    try {
      const installedVersionOutput = execSync(`"${binaryPath}" --version`, {
        timeout: 5000,
      })
        .toString()
        .trim();
      const versionMatch = installedVersionOutput.match(/(\d+\.\d+\.\d+)/);

      if (versionMatch && versionMatch[1]) {
        const installedVersion = versionMatch[1];
        if (!semverIsNewer(VERSION, installedVersion)) {
          console.log(
            `forklaunch v${installedVersion} is already installed and up-to-date.`
          );
          if (!fs.existsSync(aliasPath)) {
            createAlias(binaryPath, aliasPath);
          }
          return Promise.resolve({ installed: false, binDir: binDir });
        }
        console.log(
          `Updating forklaunch from v${installedVersion} to v${VERSION}...`
        );
      } else {
        console.log(
          "Could not determine version of existing binary. Re-installing..."
        );
      }
    } catch (e) {
      console.warn(
        "Could not execute existing binary to check version. Re-installing..."
      );
      console.warn(`Error details: ${e.message}`);
    }

    try {
      console.log("Removing old forklaunch binary...");
      fs.unlinkSync(binaryPath);
      if (fs.existsSync(aliasPath)) {
        fs.unlinkSync(aliasPath);
      }
    } catch (unlinkError) {
      console.error(`Failed to remove existing binary: ${unlinkError.message}`);
      console.error("Please check file permissions and try again.");
      return Promise.reject(unlinkError);
    }
  }

  try {
    if (!fs.existsSync(binDir)) {
      fs.mkdirSync(binDir, { recursive: true });
    }
  } catch (err) {
    if (err.code === "EACCES") {
      console.error(`Permission denied to create directory: ${binDir}`);
      console.error(
        'Please try running the installation with administrator privileges (e.g., using "sudo").'
      );
      return Promise.reject(err);
    }
    return Promise.reject(err);
  }

  console.log(`Downloading forklaunch v${VERSION} for ${platform}...`);
  console.log(`Installing to: ${binDir}`);
  console.log(`URL: ${downloadUrl}`);

  return new Promise((resolve, reject) => {
    const makeRequest = (url) => {
      const request = https
        .get(url, { agent: false }, (response) => {
          if (
            response.statusCode >= 300 &&
            response.statusCode < 400 &&
            response.headers.location
          ) {
            response.resume();
            makeRequest(response.headers.location);
            return;
          }

          if (response.statusCode !== 200) {
            reject(
              new Error(
                `Failed to download: ${response.statusCode} - ${response.statusMessage}`
              )
            );
            return;
          }

          const totalSize = parseInt(response.headers["content-length"], 10);
          const showProgressBar = !isNaN(totalSize);
          let downloadedSize = 0;
          const progressBarWidth = 40;

          response.on("data", (chunk) => {
            downloadedSize += chunk.length;
            if (showProgressBar) {
              const percentage = Math.floor((downloadedSize / totalSize) * 100);
              const completedWidth = Math.round(
                (progressBarWidth * downloadedSize) / totalSize
              );
              const remainingWidth = progressBarWidth - completedWidth;
              const bar = `[${"=".repeat(completedWidth)}${" ".repeat(remainingWidth)}]`;
              process.stdout.write(`\r${bar} ${percentage}%`);
            }
          });

          pipeline(response, fs.createWriteStream(binaryPath), (err) => {
            if (showProgressBar) {
              process.stdout.write("\n");
            }
            if (err) {
              if (err.code === "EACCES") {
                console.error(`Permission denied to write to ${binaryPath}`);
                console.error(
                  "Please try running the installation with administrator privileges."
                );
              }
              fs.unlink(binaryPath, () => {});
              return reject(err);
            }

            fs.chmodSync(binaryPath, 0o755);

            createAlias(binaryPath, aliasPath);

            resolve({ installed: true, binDir: binDir });
          });
        })
        .on("error", (err) => {
          fs.unlink(binaryPath, () => {});
          reject(err);
        });
    };
    makeRequest(downloadUrl);
  });
}

function waitForAsyncOperations() {
  return new Promise((resolve) => {
    const checkOperations = () => {
      const pendingOperations = process._getActiveRequests
        ? process._getActiveRequests()
        : [];
      const pendingHandles = process._getActiveHandles
        ? process._getActiveHandles().filter((handle) => {
            return (
              handle.constructor.name !== "WriteStream" ||
              (handle !== process.stdout && handle !== process.stderr)
            );
          })
        : [];

      if (pendingOperations.length === 0 && pendingHandles.length === 0) {
        resolve();
      } else {
        setTimeout(checkOperations, 10);
      }
    };

    setTimeout(checkOperations, 10);
  });
}

function flushOutputStreams() {
  return new Promise((resolve) => {
    let flushed = 0;
    const totalStreams = 2;

    const checkComplete = () => {
      flushed++;
      if (flushed >= totalStreams) {
        resolve();
      }
    };

    if (process.stdout.write("")) {
      checkComplete();
    } else {
      process.stdout.once("drain", checkComplete);
    }

    if (process.stderr.write("")) {
      checkComplete();
    } else {
      process.stderr.once("drain", checkComplete);
    }
  });
}

async function main() {
  try {
    const { installed, binDir } = await downloadBinary();

    if (binDir) {
      updatePathVariable(binDir);
      process.env.PATH = `${binDir}${path.delimiter}${process.env.PATH}`;
    }

    if (installed) {
      console.log("forklaunch installed successfully");

      if (process.platform !== "win32") {
        const shell = process.env.SHELL || "";
        let profile = "";
        if (shell.includes("zsh")) {
          profile = "~/.zshrc";
        } else if (shell.includes("bash")) {
          profile = "~/.bash_profile or ~/.bashrc";
        }

        if (profile) {
          console.log(
            `\nTo make the 'forklaunch' command available, please run:`
          );
          console.log(`  source ${profile}`);
          console.log(`\nAlternatively, open a new terminal window.`);
        } else {
          console.log(
            "\nPlease restart your terminal for the changes to take effect."
          );
        }
      } else {
        console.log(
          "\nPlease open a new terminal for the changes to take effect."
        );
      }
    }

    // await flushOutputStreams();
    // await waitForAsyncOperations();

    process.exit(0);
  } catch (error) {
    console.error("\nDownload failed:", error.message);
    console.error(
      "Could not download forklaunch binary. Please check your network connection or if a binary for your platform is available."
    );

    await flushOutputStreams();
    process.exit(1);
  }
}

main();
