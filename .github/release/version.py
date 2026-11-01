#!/usr/bin/env python3
"""Opencord's release versions (docs/releasing.md).

    version.py next  --branch beta          # what this push releases as
    version.py notes --version 0.0.2-beta.3 # its release notes, Markdown
    version.py stamp 0.0.2-beta.3           # writes it into Cargo and the app
    version.py manifest --version 0.0.2-beta.3 dist/  # release.json, SHA256SUMS

The version comes from the git tags and the Conventional Commits since the
last stable release, so nobody bumps it by hand. Run from anywhere inside the
repository, with every tag fetched.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The branch each channel releases from, least to most finished. SemVer
# orders the pre-release names alphabetically, and beta < canary < (none)
# happens to be this order too, so a plain SemVer comparison never offers
# anyone an older build.
CHANNELS = {"beta": "beta", "canary": "canary", "main": "stable"}
PRERELEASE_RANK = {"beta": 0, "canary": 1, "stable": 2}

TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-(beta|canary)\.(\d+))?$")
HEADER = re.compile(r"^(?P<type>[a-z]+)(?:\((?P<scope>[^)]*)\))?(?P<bang>!)?: (?P<subject>.+)$")
RELEASE_AS = re.compile(r"^Release-As:\s*v?(\d+)\.(\d+)\.(\d+)\s*$", re.MULTILINE)
BREAKING = re.compile(r"^BREAKING[ -]CHANGE:", re.MULTILINE)


@dataclass(frozen=True, order=True)
class Version:
    major: int
    minor: int
    patch: int
    rank: int = PRERELEASE_RANK["stable"]
    number: int = 0

    @property
    def channel(self) -> str:
        return next(name for name, rank in PRERELEASE_RANK.items() if rank == self.rank)

    @property
    def core(self) -> "Version":
        return Version(self.major, self.minor, self.patch)

    def __str__(self) -> str:
        core = f"{self.major}.{self.minor}.{self.patch}"
        return core if self.channel == "stable" else f"{core}-{self.channel}.{self.number}"

    @property
    def tag(self) -> str:
        return f"v{self}"

    @staticmethod
    def parse(text: str) -> "Version | None":
        match = TAG.match(text if text.startswith("v") else f"v{text}")
        if not match:
            return None
        major, minor, patch, channel, number = match.groups()
        rank = PRERELEASE_RANK[channel or "stable"]
        return Version(int(major), int(minor), int(patch), rank, int(number or 0))


@dataclass(frozen=True)
class Commit:
    sha: str
    subject: str
    body: str

    @property
    def header(self) -> "re.Match[str] | None":
        return HEADER.match(self.subject)

    @property
    def kind(self) -> str:
        header = self.header
        return header["type"] if header else "other"

    @property
    def breaking(self) -> bool:
        header = self.header
        return bool(header and header["bang"]) or bool(BREAKING.search(self.body))


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def tags() -> dict[Version, str]:
    """Every release tag, with the commit it points at."""
    found = {}
    for line in git("tag", "--list", "v*", "--format=%(refname:short) %(objectname) %(*objectname)").splitlines():
        name, sha, peeled = (line.split() + [""])[:3]
        version = Version.parse(name)
        if version:
            found[version] = peeled or sha
    return found


def commits(since: str | None) -> list[Commit]:
    """Commits since `since` (all of them without it), merges left out."""
    span = f"{since}..HEAD" if since else "HEAD"
    log = git("log", "--no-merges", "--format=%H%x1f%s%x1f%b%x1e", span)
    out = []
    for record in log.split("\x1e"):
        record = record.strip("\n")
        if record:
            sha, subject, body = record.split("\x1f")
            out.append(Commit(sha, subject.strip(), body))
    return out


def bump(base: Version, changes: list[Commit]) -> Version:
    """The next stable version after `base` for these changes.

    Before 1.0.0 every step is one smaller (SemVer §4: anything may change):
    a breaking change bumps the minor, anything else the patch. A commit's
    `Release-As: x.y.z` footer overrides that, to start 1.0.0 for one.
    """
    pinned = [Version(*map(int, m)) for c in changes for m in RELEASE_AS.findall(c.body)]
    if pinned and max(pinned) > base:
        return max(pinned)
    breaking = any(c.breaking for c in changes)
    feature = any(c.kind == "feat" for c in changes)
    if base.major == 0:
        if breaking:
            return Version(0, base.minor + 1, 0)
        return Version(0, base.minor, base.patch + 1)
    if breaking:
        return Version(base.major + 1, 0, 0)
    if feature:
        return Version(base.major, base.minor + 1, 0)
    return Version(base.major, base.minor, base.patch + 1)


def next_version(branch: str) -> dict:
    channel = CHANNELS.get(branch)
    if channel is None:
        raise SystemExit(f"{branch} releases nothing: only {', '.join(CHANNELS)} do")
    released = tags()
    head = git("rev-parse", "HEAD").strip()

    # A re-run of a release this commit already made keeps its version.
    mine = [v for v, sha in released.items() if sha == head and v.channel == channel]
    if mine:
        return describe(max(mine), existing=True)

    stable = [v for v in released if v.channel == "stable"]
    base = max(stable, default=Version(0, 0, 0))
    changes = commits(base.tag if stable else None)
    if not changes:
        return {"release": "false", "channel": channel, "reason": f"nothing since {base.tag}"}
    # Before the first stable release everything works towards 0.0.1, unless
    # a commit pins another with Release-As.
    if not stable:
        changes = [c for c in changes if RELEASE_AS.search(c.body)]
    core = bump(base, changes)
    if channel == "stable":
        version = core
        if version in released:
            raise SystemExit(f"{version.tag} is already released from another commit")
    else:
        rank = PRERELEASE_RANK[channel]
        numbers = [v.number for v in released if v.core == core and v.rank == rank]
        version = Version(core.major, core.minor, core.patch, rank, max(numbers, default=0) + 1)
    return describe(version, existing=False)


def describe(version: Version, existing: bool) -> dict:
    channel = version.channel
    return {
        "release": "true",
        "existing": str(existing).lower(),
        "version": str(version),
        "core": str(version.core),
        "tag": version.tag,
        "channel": channel,
        "prerelease": str(channel != "stable").lower(),
        "docker_tags": ",".join(docker_tags(version)),
    }


def docker_tags(version: Version) -> list[str]:
    if version.channel != "stable":
        return [str(version), version.channel]
    tags = [str(version), f"{version.major}.{version.minor}", "latest"]
    if version.major > 0:
        tags.insert(2, str(version.major))
    return tags


def previous_tag(version: Version) -> str | None:
    """Where this release's notes start: the last release of its channel
    before it that this commit contains, else the last stable release."""
    released = tags()
    older = sorted((v for v in released if v < version), reverse=True)
    for candidate in older:
        if candidate.channel in (version.channel, "stable"):
            contained = subprocess.run(
                ["git", "merge-base", "--is-ancestor", candidate.tag, "HEAD"], cwd=ROOT
            ).returncode == 0
            if contained or candidate.channel == "stable":
                return candidate.tag
    return None


SECTIONS = [
    ("Breaking changes", lambda c: c.breaking),
    ("Features", lambda c: c.kind == "feat"),
    ("Fixes", lambda c: c.kind == "fix"),
    ("Performance", lambda c: c.kind == "perf"),
    ("Other changes", lambda c: True),
]


def notes(version_text: str) -> str:
    version = Version.parse(version_text)
    if version is None:
        raise SystemExit(f"{version_text} is not a release version")
    since = previous_tag(version)
    changes = commits(since)
    lines = []
    left = list(changes)
    for title, belongs in SECTIONS:
        mine = [c for c in left if belongs(c)]
        left = [c for c in left if c not in mine]
        if not mine:
            continue
        lines += [f"### {title}", ""]
        for c in mine:
            header = c.header
            text = header["subject"] if header else c.subject
            scope = f"**{header['scope']}:** " if header and header["scope"] else ""
            lines.append(f"- {scope}{text} ({c.sha[:7]})")
        lines.append("")
    if not changes:
        lines += ["No changes since the last release.", ""]
    if since:
        repo = os.environ.get("GITHUB_REPOSITORY", "mrafieefard/opencord")
        lines.append(f"**Full changelog:** https://github.com/{repo}/compare/{since}...{version.tag}")
    return "\n".join(lines).rstrip() + "\n"


def replace(path: Path, pattern: str, replacement: str, count: int = 1) -> None:
    text = path.read_text()
    new, done = re.subn(pattern, replacement, text, count=count, flags=re.MULTILINE)
    if done == 0:
        raise SystemExit(f"{path.relative_to(ROOT)}: found nothing to stamp")
    path.write_text(new)


def stamp(version_text: str, build: int | None) -> None:
    """Writes the version wherever the builds read it from."""
    if Version.parse(version_text) is None:
        raise SystemExit(f"{version_text} is not a release version")
    replace(
        ROOT / "Cargo.toml",
        r'(\[workspace\.package\]\n(?:[^\[\n].*\n)*?version = )"[^"]*"',
        rf'\g<1>"{version_text}"',
    )
    # Cargo.lock lists the workspace's own crates with that version too;
    # builds run --locked, so it has to agree.
    members = set()
    for manifest in (ROOT / "crates").glob("*/Cargo.toml"):
        name = re.search(r'^name = "([^"]+)"', manifest.read_text(), re.MULTILINE)
        if name:
            members.add(name[1])
    lock = ROOT / "Cargo.lock"
    text = lock.read_text()
    for name in members:
        text, done = re.subn(
            rf'(\[\[package\]\]\nname = "{re.escape(name)}"\nversion = )"[^"]*"(\n(?!source))',
            rf'\g<1>"{version_text}"\g<2>',
            text,
        )
        if done != 1:
            raise SystemExit(f"Cargo.lock: {name} found {done} times")
    lock.write_text(text)
    pubspec = ROOT / "app" / "pubspec.yaml"
    current_build = re.search(r"^version: [^+\s]+\+(\d+)", pubspec.read_text(), re.MULTILINE)
    number = build if build is not None else int(current_build[1]) if current_build else 1
    replace(pubspec, r"^version: .*$", f"version: {version_text}+{number}")
    replace(
        ROOT / "app" / "lib" / "core" / "app_info.dart",
        r"^const appVersion = '[^']*';",
        f"const appVersion = '{version_text}';",
    )


# What each release file is, by the name the updater looks it up with
# (docs/releasing.md, "For the updater"). The names carry no version, so
# releases/latest/download/<file> always reaches the newest stable one.
ASSETS = {
    "opencord-linux-x64.tar.gz": "client-linux-x64",
    "opencord-windows-x64.zip": "client-windows-x64",
    "opencord-macos.zip": "client-macos-universal",
    "opencord-setup-linux-x64": "setup-linux-x64",
    "opencord-setup-windows-x64.exe": "setup-windows-x64",
    "opencord-setup-macos.dmg": "setup-macos-universal",
    "opencord-server-linux-x64.tar.gz": "server-linux-x64",
    "opencord-server-windows-x64.zip": "server-windows-x64",
    "opencord-server-macos.tar.gz": "server-macos-universal",
    "opencord-android.apk": "android-apk",
    "opencord-ios-unsigned.ipa": "ios-ipa-unsigned",
}

# Release files a release may go without: the APK is there only when it was
# signed with Opencord's key (ANDROID_KEYSTORE_BASE64).
OPTIONAL = {"android-apk"}


def manifest(version_text: str, dist: Path, image: str | None) -> None:
    """Writes `release.json` and `SHA256SUMS` beside the release files."""
    version = Version.parse(version_text)
    if version is None:
        raise SystemExit(f"{version_text} is not a release version")
    repo = os.environ.get("GITHUB_REPOSITORY", "mrafieefard/opencord")
    base = f"https://github.com/{repo}/releases/download/{version.tag}"
    platforms = {}
    sums = []
    for path in sorted(dist.iterdir()):
        if not path.is_file() or path.name in ("release.json", "release.json.sig", "SHA256SUMS"):
            continue
        key = ASSETS.get(path.name)
        if key is None:
            raise SystemExit(f"{path.name} is not a known release file: add it to ASSETS")
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.name}")
        platforms[key] = {
            "file": path.name,
            "url": f"{base}/{path.name}",
            "size": path.stat().st_size,
            "sha256": digest,
        }
    missing = sorted(set(ASSETS.values()) - set(platforms) - OPTIONAL)
    if missing:
        raise SystemExit(f"missing release files: {', '.join(missing)}")
    data = {
        "schema": 1,
        "version": str(version),
        "channel": version.channel,
        "tag": version.tag,
        "commit": os.environ.get("GITHUB_SHA") or git("rev-parse", "HEAD").strip(),
        "published_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "notes_url": f"https://github.com/{repo}/releases/tag/{version.tag}",
        "platforms": platforms,
    }
    if image:
        data["docker"] = {"image": image, "tags": docker_tags(version)}
    (dist / "release.json").write_text(json.dumps(data, indent=2) + "\n")
    (dist / "SHA256SUMS").write_text("\n".join(sums) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description="Opencord's release versions")
    sub = parser.add_subparsers(dest="command", required=True)
    nxt = sub.add_parser("next", help="the version this branch releases as now")
    nxt.add_argument("--branch", required=True)
    nxt.add_argument("--github-output", action="store_true", help="also write to $GITHUB_OUTPUT")
    nts = sub.add_parser("notes", help="release notes since the previous release")
    nts.add_argument("--version", required=True)
    stm = sub.add_parser("stamp", help="write a version into Cargo and the app")
    stm.add_argument("version")
    stm.add_argument("--build", type=int, help="the app's build number")
    man = sub.add_parser("manifest", help="release.json and SHA256SUMS for a release's files")
    man.add_argument("--version", required=True)
    man.add_argument("--image", help="the server's Docker image, without a tag")
    man.add_argument("dist", type=Path)
    args = parser.parse_args()

    if args.command == "next":
        result = next_version(args.branch)
        print(json.dumps(result, indent=2))
        if args.github_output:
            with open(os.environ["GITHUB_OUTPUT"], "a") as out:
                for key, value in result.items():
                    out.write(f"{key}={value}\n")
    elif args.command == "notes":
        sys.stdout.write(notes(args.version))
    elif args.command == "manifest":
        manifest(args.version, args.dist, args.image)
    else:
        stamp(args.version, args.build)


if __name__ == "__main__":
    main()
