"""Cartridge CLI: run and scaffold cartridge apps."""

from __future__ import annotations

import argparse
import asyncio
import importlib.util
import inspect
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(prog="cartridge", description="Cartridge SDK CLI")
    subparsers = parser.add_subparsers(dest="command")

    # cartridge run
    run_parser = subparsers.add_parser("run", help="Run a cartridge in desktop simulation mode")
    run_parser.add_argument("--resolution", default="640x480", help="Window resolution WxH")
    run_parser.add_argument("--fullscreen", action="store_true")
    run_parser.add_argument("--path", default=".", help="Path to cartridge directory")

    # cartridge new
    new_parser = subparsers.add_parser("new", help="Create a new cartridge project")
    new_parser.add_argument("name", help="Project name")

    # cartridge home
    home_parser = subparsers.add_parser("home", help="Run the launcher home screen loop")
    home_parser.add_argument("path", help="Path to the client cartridge directory")
    home_parser.add_argument("--resolution", default="640x480", help="Window resolution WxH")
    home_parser.add_argument("--fullscreen", action="store_true")

    # cartridge install
    install_parser = subparsers.add_parser("install", help="Install a cartridge from GitHub")
    install_parser.add_argument("url", help="GitHub repository URL")
    install_parser.add_argument("--branch", default="main", help="Branch to install from")

    args = parser.parse_args()

    if args.command == "run":
        _run_cartridge(args)
    elif args.command == "new":
        _scaffold_cartridge(args)
    elif args.command == "home":
        _home_loop(args)
    elif args.command == "install":
        _install_cartridge(args)
    else:
        parser.print_help()


def _parse_resolution(resolution: str) -> tuple[int, int]:
    w, h = 640, 480
    if resolution:
        parts = resolution.lower().split("x")
        if len(parts) == 2:
            w, h = int(parts[0]), int(parts[1])
    return w, h


def _run_cartridge_at_path(
    cart_dir: Path, w: int = 640, h: int = 480, fullscreen: bool = False,
) -> None:
    from cartridge_sdk.app import CartridgeApp
    from cartridge_sdk.manifest import AppManifest
    from cartridge_sdk.runner import CartridgeRunner

    manifest = AppManifest.from_dir(cart_dir)

    # Import the entry point module
    entry_path = cart_dir / manifest.entry_point
    if not entry_path.exists():
        print(f"Entry point not found: {entry_path}")
        sys.exit(1)

    # Add cartridge src dir to path so internal imports work
    src_dir = entry_path.parent
    if str(src_dir) not in sys.path:
        sys.path.insert(0, str(src_dir))
    # Also add parent of src for package-style imports
    if str(src_dir.parent) not in sys.path:
        sys.path.insert(0, str(src_dir.parent))

    spec = importlib.util.spec_from_file_location("cartridge_main", entry_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    # Find the CartridgeApp subclass
    app_class = None
    for name, obj in inspect.getmembers(module, inspect.isclass):
        if issubclass(obj, CartridgeApp) and obj is not CartridgeApp:
            app_class = obj
            break

    if app_class is None:
        print(f"No CartridgeApp subclass found in {entry_path}")
        sys.exit(1)

    app = app_class()
    runner = CartridgeRunner(app, manifest, w, h, fullscreen=fullscreen)

    print(f"Running: {manifest.name} v{manifest.version}")
    asyncio.run(runner.run())


def _run_cartridge(args) -> None:
    cart_dir = Path(args.path).resolve()
    w, h = _parse_resolution(args.resolution)
    _run_cartridge_at_path(cart_dir, w, h, fullscreen=args.fullscreen)


def _home_loop(args) -> None:
    """EmulationStation-style process loop: run client, check .launch, run target, repeat."""
    from cartridge_sdk.management import LAUNCH_FILE

    client_dir = Path(args.path).resolve()
    w, h = _parse_resolution(args.resolution)

    res_arg = f"{w}x{h}"
    fs_args = ["--fullscreen"] if args.fullscreen else []

    while True:
        # Clean up any stale launch file
        if LAUNCH_FILE.exists():
            LAUNCH_FILE.unlink()

        # Run the client app as a subprocess
        result = subprocess.run(
            [sys.executable, "-m", "cartridge_sdk.cli.main", "run",
             "--path", str(client_dir), "--resolution", res_arg] + fs_args,
        )

        # Check if a launch was requested
        if LAUNCH_FILE.exists():
            target_path = LAUNCH_FILE.read_text().strip()
            LAUNCH_FILE.unlink()

            if target_path and Path(target_path).exists():
                print(f"Launching: {target_path}")
                subprocess.run(
                    [sys.executable, "-m", "cartridge_sdk.cli.main", "run",
                     "--path", target_path, "--resolution", res_arg] + fs_args,
                )
                # After target exits, loop back to client
                continue

        # No launch file or client exited normally — stop the loop
        break


def _install_cartridge(args) -> None:
    from cartridge_sdk.management import install_from_github

    try:
        manifest = install_from_github(args.url, branch=args.branch)
        print(f"Installed: {manifest.name} v{manifest.version} ({manifest.id})")
    except Exception as e:
        print(f"Install failed: {e}")
        sys.exit(1)


def _scaffold_cartridge(args) -> None:
    name = args.name
    slug = name.lower().replace(" ", "_").replace("-", "_")
    project_dir = Path.cwd() / slug

    if project_dir.exists():
        print(f"Directory already exists: {project_dir}")
        sys.exit(1)

    # Create structure
    (project_dir / "src" / "screens").mkdir(parents=True)
    (project_dir / "assets").mkdir()

    # cartridge.toml
    toml = f"""[app]
id = "dev.cartridge.{slug}"
name = "{name}"
description = ""
version = "0.1.0"
author = ""

[app.entry]
main = "src/main.py"

[permissions]
network = false
storage = false
"""
    (project_dir / "cartridge.toml").write_text(toml)

    # src/main.py
    main_py = f"""\"\"\"
{name} - A Cartridge app.
\"\"\"
import asyncio
from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent, Button


class {_to_class_name(name)}App(CartridgeApp):

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)

    def on_input(self, event: InputEvent) -> None:
        pass

    async def on_update(self, dt: float) -> None:
        pass

    def on_render(self, screen: Screen) -> None:
        screen.clear()
        screen.draw_text("{name}", 20, 20, bold=True, font_size=24)
        screen.draw_text("Press Esc to quit", 20, 60, color=screen.theme.text_dim)
"""
    (project_dir / "src" / "main.py").write_text(main_py)

    # screens/__init__.py
    (project_dir / "src" / "screens" / "__init__.py").write_text("")

    print(f"Created new cartridge: {project_dir}")
    print(f"  cd {slug}")
    print(f"  cartridge run")


def _to_class_name(name: str) -> str:
    return "".join(word.capitalize() for word in name.replace("-", " ").replace("_", " ").split())


if __name__ == "__main__":
    main()
