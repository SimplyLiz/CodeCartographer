#!/usr/bin/env python3
"""
Nyx.Navigator Launcher
Build, install, and run Nyx.Navigator interactively.
"""

import os
import platform
import shutil
import subprocess
import sys

# ── Colours ──────────────────────────────────────────────────────────────────

class C:
    HEADER = '\033[95m'
    BLUE   = '\033[94m'
    CYAN   = '\033[96m'
    GREEN  = '\033[92m'
    YELLOW = '\033[93m'
    RED    = '\033[91m'
    END    = '\033[0m'
    BOLD   = '\033[1m'
    DIM    = '\033[2m'

def ok(msg):      print(f"{C.GREEN}✓ {msg}{C.END}")
def err(msg):     print(f"{C.RED}✗ {msg}{C.END}")
def info(msg):    print(f"{C.YELLOW}ℹ {msg}{C.END}")
def section(title): print(f"\n{C.BOLD}{C.BLUE}▶ {title}{C.END}")

def banner():
    print(f"\n{C.BOLD}{C.CYAN}{'='*60}")
    print("  Nyx.Navigator — Launcher")
    print(f"{'='*60}{C.END}\n")

def confirm(prompt, default_yes=False):
    hint = "Y/n" if default_yes else "y/N"
    try:
        resp = input(f"{C.YELLOW}{prompt} ({hint}): {C.END}").strip().lower()
    except KeyboardInterrupt:
        print()
        sys.exit(0)
    return (resp == "" and default_yes) or resp in ("y", "yes")

def choose(prompt, options):
    print(f"\n{C.BOLD}{prompt}{C.END}")
    keys = list(options.keys())
    for i, k in enumerate(keys, 1):
        print(f"  {C.BOLD}{i}{C.END}) {options[k]}")
    while True:
        try:
            raw = input(f"\n{C.YELLOW}Enter number: {C.END}").strip()
        except KeyboardInterrupt:
            print()
            sys.exit(0)
        if raw.isdigit() and 1 <= int(raw) <= len(keys):
            return keys[int(raw) - 1]
        print("  Invalid choice, try again.")

# ── Paths ─────────────────────────────────────────────────────────────────────

SCRIPT_DIR  = os.path.dirname(os.path.abspath(__file__))
CARGO_DIR   = os.path.join(SCRIPT_DIR, "mapper-core", "navigator")
BINARY_NAME = "navigator"

def binary_src():
    ext = ".exe" if platform.system() == "Windows" else ""
    return os.path.join(CARGO_DIR, "target", "release", f"{BINARY_NAME}{ext}")

def install_dir():
    if platform.system() == "Windows":
        base = os.environ.get("LOCALAPPDATA", os.path.expanduser("~"))
        return os.path.join(base, "Programs", BINARY_NAME)
    return os.path.join(os.path.expanduser("~"), ".local", "bin")

def find_installed_binary():
    """Return path to installed binary, or None."""
    return shutil.which(BINARY_NAME)

# ── Core helpers ─────────────────────────────────────────────────────────────

def check_cargo():
    if not shutil.which("cargo"):
        err("Rust not found.")
        info("Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")
        sys.exit(1)
    result = subprocess.run(["cargo", "--version"], capture_output=True, text=True)
    ok(result.stdout.strip())

def build():
    section("Building Nyx.Navigator")
    info(f"cargo build --release  (in {CARGO_DIR})")
    result = subprocess.run(["cargo", "build", "--release"], cwd=CARGO_DIR)
    if result.returncode != 0:
        err("Build failed.")
        sys.exit(1)
    ok("Build complete")
    return binary_src()

def install_binary():
    src = binary_src()
    if not os.path.exists(src):
        err("Binary not found — build first.")
        return

    idir = install_dir()
    os.makedirs(idir, exist_ok=True)
    ext   = ".exe" if platform.system() == "Windows" else ""
    dest  = os.path.join(idir, f"{BINARY_NAME}{ext}")
    shutil.copy2(src, dest)
    if platform.system() != "Windows":
        os.chmod(dest, 0o755)
    ok(f"Installed: {dest}")
    _ensure_path(idir)

def _ensure_path(idir):
    if platform.system() == "Windows":
        info(f"Add to PATH manually: {idir}")
        return
    export_line = f'export PATH="{idir}:$PATH"'
    shell = os.environ.get("SHELL", "")
    candidates = (
        [os.path.expanduser("~/.zshrc"), os.path.expanduser("~/.bashrc")]
        if "zsh" in shell else
        [os.path.expanduser("~/.bashrc"), os.path.expanduser("~/.zshrc")]
    )
    for rc in candidates:
        if os.path.exists(rc):
            with open(rc) as f:
                content = f.read()
            if idir in content:
                ok(f"PATH already set in {rc}")
                return
            with open(rc, "a") as f:
                f.write(f"\n{export_line}\n")
            ok(f"PATH updated in {rc}")
            return
    with open(candidates[0], "a") as f:
        f.write(f"\n{export_line}\n")
    ok(f"PATH updated in {candidates[0]}")

def verify():
    binary = find_installed_binary() or binary_src()
    result = subprocess.run([binary, "--version"], capture_output=True, text=True)
    if result.returncode == 0:
        ok(result.stdout.strip())
    else:
        info("Binary not reachable on PATH — restart terminal or use full path.")

# ── Run helpers ───────────────────────────────────────────────────────────────

def _navigator(args, cwd=None):
    binary = find_installed_binary() or binary_src()
    if not os.path.exists(binary) and not shutil.which(binary):
        err("navigator binary not found — build and install first.")
        return
    subprocess.run([binary] + args, cwd=cwd or SCRIPT_DIR)

def ask_target_path():
    try:
        raw = input(f"{C.YELLOW}  Target path (leave blank for current dir): {C.END}").strip()
    except KeyboardInterrupt:
        print()
        sys.exit(0)
    return raw or None

# ── Actions ───────────────────────────────────────────────────────────────────

def action_build_install():
    section("Checking Rust")
    check_cargo()
    build()
    section("Installing binary")
    install_binary()
    section("Verifying")
    verify()

def action_build_only():
    section("Checking Rust")
    check_cargo()
    build()
    ok(f"Binary at: {binary_src()}")

def action_map():
    section("Run: navigator map")
    path = ask_target_path()
    args = ["map"] + ([path] if path else [])
    _navigator(args)

def action_health():
    section("Run: navigator health")
    path = ask_target_path()
    args = ["health"] + ([path] if path else [])
    _navigator(args)

def action_serve():
    section("Run: navigator serve  (MCP server — Ctrl+C to stop)")
    path = ask_target_path()
    args = ["serve"] + ([path] if path else [])
    _navigator(args)

def action_init():
    section("Run: navigator init")
    path = ask_target_path()
    args = ["init"] + ([path] if path else [])
    _navigator(args)

def action_watch():
    section("Run: navigator watch  (Ctrl+C to stop)")
    path = ask_target_path()
    args = ["watch"] + ([path] if path else [])
    _navigator(args)

def action_verify():
    section("Verify installation")
    verify()

# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    os.chdir(SCRIPT_DIR)
    banner()

    menu = {
        "install": "Build and install navigator globally",
        "build":   "Build only (no install)",
        "init":    "Run: navigator init   — create .navigator/config.toml",
        "map":     "Run: navigator map   — generate skeleton map",
        "health":  "Run: navigator health — architectural health score",
        "serve":   "Run: navigator serve  — start MCP server",
        "watch":   "Run: navigator watch  — live file watcher",
        "verify":  "Verify installed binary",
        "quit":    "Quit",
    }

    action = choose("What would you like to do?", menu)

    if action == "install":
        action_build_install()
    elif action == "build":
        action_build_only()
    elif action == "init":
        action_init()
    elif action == "map":
        action_map()
    elif action == "health":
        action_health()
    elif action == "serve":
        action_serve()
    elif action == "watch":
        action_watch()
    elif action == "verify":
        action_verify()
    elif action == "quit":
        info("Bye.")
        sys.exit(0)

    print()

if __name__ == "__main__":
    main()
