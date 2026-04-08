#!/usr/bin/env python3
"""
Cartographer Installation Script
Builds and installs the cartographer binary for Linux, macOS, and Windows.
"""

import os
import platform
import shutil
import subprocess
import sys

BINARY_NAME = "cartographer"
CARGO_DIR = os.path.join("mapper-core", "cartographer")


def step(n: int, total: int, msg: str):
    print(f"[{n}/{total}] {msg}...")


def check_cargo():
    if not shutil.which("cargo"):
        print("Rust not found. Install it first:")
        print("  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")
        sys.exit(1)
    print("  Rust found")


def build():
    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=CARGO_DIR,
    )
    if result.returncode != 0:
        print("Build failed.")
        sys.exit(1)
    print("  Build successful")


def get_binary_src() -> str:
    if platform.system() == "Windows":
        return os.path.join(CARGO_DIR, "target", "release", f"{BINARY_NAME}.exe")
    return os.path.join(CARGO_DIR, "target", "release", BINARY_NAME)


def get_install_dir() -> str:
    if platform.system() == "Windows":
        local_app = os.environ.get("LOCALAPPDATA", os.path.expanduser("~"))
        return os.path.join(local_app, "Programs", BINARY_NAME)
    return os.path.join(os.path.expanduser("~"), ".local", "bin")


def install_binary(src: str, install_dir: str) -> str:
    os.makedirs(install_dir, exist_ok=True)
    dest_name = f"{BINARY_NAME}.exe" if platform.system() == "Windows" else BINARY_NAME
    dest = os.path.join(install_dir, dest_name)
    shutil.copy2(src, dest)
    if platform.system() != "Windows":
        os.chmod(dest, 0o755)
    print(f"  Binary installed: {dest}")
    return install_dir


def update_path(install_dir: str):
    system = platform.system()

    if system == "Windows":
        # Inform the user; modifying system PATH on Windows requires elevation
        print(f"  Add to PATH manually: {install_dir}")
        print("  Or run: [System.Environment]::SetEnvironmentVariable('PATH', $env:PATH + ';{install_dir}', 'User')")
        return

    export_line = f'export PATH="{install_dir}:$PATH"'
    shell = os.environ.get("SHELL", "")
    candidates = []
    if "zsh" in shell:
        candidates = [os.path.expanduser("~/.zshrc"), os.path.expanduser("~/.bashrc")]
    else:
        candidates = [os.path.expanduser("~/.bashrc"), os.path.expanduser("~/.zshrc")]

    for rc in candidates:
        if os.path.exists(rc):
            with open(rc, "r") as f:
                content = f.read()
            if install_dir in content:
                print(f"  PATH already set in {rc}")
                return
            with open(rc, "a") as f:
                f.write(f"\n{export_line}\n")
            print(f"  PATH updated in {rc}")
            return

    # Fallback: write to the first candidate
    rc = candidates[0]
    with open(rc, "a") as f:
        f.write(f"\n{export_line}\n")
    print(f"  PATH updated in {rc}")


def verify(install_dir: str):
    dest_name = f"{BINARY_NAME}.exe" if platform.system() == "Windows" else BINARY_NAME
    binary_path = os.path.join(install_dir, dest_name)
    result = subprocess.run([binary_path, "--version"], capture_output=True, text=True)
    if result.returncode == 0:
        print(f"  {result.stdout.strip()}")
    else:
        print("  Restart your terminal for PATH changes to take effect")


def main():
    print("=" * 48)
    print("  Cartographer Installation")
    print("=" * 48)
    print()

    total = 4
    step(1, total, "Checking Rust")
    check_cargo()
    print()

    step(2, total, "Building Cartographer (this may take a few minutes)")
    build()
    print()

    step(3, total, "Installing")
    src = get_binary_src()
    install_dir = get_install_dir()
    install_binary(src, install_dir)
    update_path(install_dir)
    print()

    step(4, total, "Verifying")
    verify(install_dir)
    print()

    print("=" * 48)
    print("  Installation complete!")
    print("=" * 48)
    print()
    print("Next steps:")
    print("  1. Restart your terminal (if needed)")
    print("  2. Set your UltraContext API key:")
    print("       cartographer init --cloud --project my-project")
    print("  3. Generate your first context:")
    print("       cartographer source")
    print("  4. Push to cloud:")
    print("       cartographer push")
    print("  5. Start MCP server:")
    print("       cartographer serve")
    print()


if __name__ == "__main__":
    main()
