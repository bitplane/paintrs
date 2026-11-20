"""Package a release binary built on the target desktop OS."""

import argparse
import hashlib
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import tomllib
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("linux-x86_64", "windows-x86_64", "macos-aarch64", "macos-x86_64"))
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()

    with open("Cargo.toml", "rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]

    desktop = args.platform.split("-", 1)[0]
    dist = Path("target/dist")
    staging = dist / args.platform
    staging.mkdir(parents=True, exist_ok=True)
    files = []
    if desktop == "macos":
        app = staging / "Paintrs.app"
        binary_dir = app / "Contents/MacOS"
        binary_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(args.binary, binary_dir / "paintrs")
        with (app / "Contents/Info.plist").open("wb") as plist:
            plistlib.dump({
                "CFBundleName": "Paintrs",
                "CFBundleIdentifier": "net.bitplane.paintrs",
                "CFBundleVersion": version,
                "CFBundleShortVersionString": version,
                "CFBundleExecutable": "paintrs",
                "CFBundlePackageType": "APPL",
                "NSHighResolutionCapable": True,
            }, plist)
        subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
        files.append((app, "Paintrs.app"))
    else:
        files.append((args.binary, "paintrs.exe" if desktop == "windows" else "paintrs"))

    files.extend((Path(name), name) for name in (
        "README.md", "LICENSE-MIT", "LICENSE-APACHE", "assets/Liberation-LICENSE.txt",
    ))
    stem = f"paintrs-{version}-{args.platform}"
    if desktop == "windows":
        archive = dist / f"{stem}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path, name in files:
                output.write(path, name)
    else:
        archive = dist / f"{stem}.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for path, name in files:
                output.add(path, arcname=name)

    with archive.open("rb") as stream:
        checksum = hashlib.file_digest(stream, "sha256").hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(
        f"{checksum}  {archive.name}\n", encoding="ascii", newline="\n",
    )
    print(archive)


if __name__ == "__main__":
    main()
