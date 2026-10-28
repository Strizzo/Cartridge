#!/usr/bin/env python3
"""Collect exact distro source archives beside a finalized private browser engine.

No extraction, builds, signing, publication, global APT changes or engine writes.
The unsigned lock describes collected provenance, not legal compliance.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import stat
import subprocess
import sys
import urllib.parse
import urllib.request

BWRAP_VERSION = "0.13.1"
BWRAP_SIZE = 129472
BWRAP_SHA256 = "0e85c8702248b8dd64144f594147553650cdd0e4c951690345faace79c217c4d"
BWRAP_URL = "https://github.com/containers/bubblewrap/releases/download/v0.13.1/bubblewrap-0.13.1.tar.xz"
LOCK = "sources.lock.json"
RESERVE = 256 * 1024**2
MAX_ARTIFACT = 2 * 1024**3
MAX_TOTAL = 4 * 1024**3
REPOS = {
    "debian": [("https://deb.debian.org/debian", "bookworm"),
               ("https://security.debian.org/debian-security", "bookworm-security")],
    "ubuntu": [("https://ports.ubuntu.com/ubuntu-ports", suite)
               for suite in ("noble", "noble-updates", "noble-security")],
}
STATIC_PACKAGES = ("libc6", "libc6-dev", "libc-bin", "libcap2", "libcap-dev")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    with path.open("rb") as stream:
        digest = hashlib.sha256()
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
        return digest.hexdigest()


def safe_path(value):
    require(isinstance(value, str) and 0 < len(value) <= 1024 and value.isascii()
            and re.fullmatch(r"[A-Za-z0-9_+.~:/%-]+", value) is not None,
            "Unsafe relative path")
    require(not value.startswith("/") and all(p not in ("", ".", "..") for p in value.split("/"))
            and "%" not in value and ":" not in value, "Unsafe relative path")
    return value


def within(root, name):
    path = root / safe_path(name)
    require(root.is_dir() and not root.is_symlink(), "Bundle must be a real directory")
    current = root
    for part in path.relative_to(root).parts:
        current = current / part
        require(not current.is_symlink(), "Symlink in bundle: " + name)
    return path


def regular(path):
    require(stat.S_ISREG(path.lstat().st_mode), "Expected regular file: " + str(path))
    return path


def file_record(root, path, **extra):
    regular(path)
    return dict(path=safe_path(path.relative_to(root).as_posix()), size=path.stat().st_size,
                sha256=sha256(path), **extra)


def check_file(root, record):
    path = regular(within(root, record["path"]))
    require(path.stat().st_size == record["size"] and sha256(path) == record["sha256"],
            "Size/checksum mismatch: " + record["path"])


def write_json(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def strict_json(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "Duplicate JSON key: " + key)
            result[key] = value
        return result
    require(path.stat().st_size <= 32 * 1024**2, "Oversized lock")
    return json.loads(regular(path).read_text(), object_pairs_hook=unique)


def parse_tsv(text, columns):
    result = []
    for line in text.splitlines():
        row = line.split("\t")
        require(len(row) == columns and all(row), "Expected tab-separated inventory")
        require(all(re.fullmatch(r"[a-zA-Z0-9+.:~_-]+", item) for item in row), "Invalid package/version")
        result.append(tuple(row))
    require(result and len(result) == len(set(result)), "Empty/duplicate inventory")
    return result


def deb822(text):
    result = []
    for paragraph in text.strip().split("\n\n"):
        fields = {}
        last = None
        for line in paragraph.splitlines():
            if line.startswith((" ", "\t")):
                require(last is not None, "Invalid source continuation")
                fields[last] += "\n" + line[1:]
            else:
                require(":" in line, "Invalid source record")
                last, value = line.split(":", 1)
                require(last not in fields, "Duplicate source field")
                fields[last] = value.strip()
        result.append((fields, paragraph + "\n"))
    return result


def source_files(fields):
    result = {}
    for line in fields.get("Checksums-Sha256", "").splitlines():
        if not line.strip():
            continue
        parts = line.split()
        require(len(parts) == 3, "Invalid source checksum record")
        digest, size, name = parts
        safe_path(name)
        require("/" not in name and name not in result and re.fullmatch(r"[a-f0-9]{64}", digest),
                "Unsafe/duplicate source filename or digest")
        require(size.isdecimal() and 0 < int(size) <= MAX_ARTIFACT, "Invalid source size")
        result[name] = dict(size=int(size), sha256=digest)
    require(len(result) >= 2 and sum(n.endswith(".dsc") for n in result) == 1,
            "Incomplete source record (descriptor and all payloads required)")
    return result


def exact_source(text, package, version, uris=None, distribution=None):
    matches = [(fields, raw) for fields, raw in deb822(text)
               if fields.get("Package") == package and fields.get("Version") == version]
    require(matches, "Exact source unavailable: " + package + "=" + version)
    files = source_files(matches[0][0])
    require(all(source_files(fields) == files for fields, _ in matches), "Conflicting exact source checksums")
    if uris is not None:
        # Identical uploads can occur in both the main and security pools.
        # Select APT's actual pool without changing version or artifact hashes.
        for fields, raw in matches:
            try:
                parse_uris(uris, distribution, fields)
                return fields, raw
            except ValueError:
                continue
        raise ValueError("APT URIs do not match any authenticated exact source record")
    return matches[0]


def official_url(url, distribution):
    parsed = urllib.parse.urlsplit(url)
    require(parsed.scheme == "https" and not parsed.username and not parsed.password
            and not parsed.query and not parsed.fragment and not parsed.port,
            "Source URL must be official HTTPS")
    bases = [base for base, _ in REPOS[distribution]]
    require(any(url.startswith(base + "/pool/") for base in bases), "Unexpected source origin")
    safe_path(urllib.parse.unquote(parsed.path).lstrip("/"))
    return url


def parse_uris(text, distribution, fields):
    files = source_files(fields)
    directory = safe_path(fields["Directory"])
    found = {}
    for line in text.splitlines():
        if not line.startswith("'"):
            continue
        parts = shlex.split(line)
        require(len(parts) == 4, "Invalid APT URI record")
        url, name, size, checksum = parts
        official_url(url, distribution)
        strong_checksums = {"SHA256:" + files.get(name, {}).get("sha256", "")}
        # Ubuntu advertises SHA512 as APT's preferred hash. Bind that value to
        # the same signed record; the sidecar always locks/download-checks SHA256.
        for entry in fields.get("Checksums-Sha512", "").splitlines():
            parts512 = entry.split()
            if len(parts512) == 3 and parts512[2] == name and parts512[1] == size:
                require(re.fullmatch(r"[a-f0-9]{128}", parts512[0]), "Invalid SHA512 source checksum")
                strong_checksums.add("SHA512:" + parts512[0])
        require(name in files and name not in found and size == str(files[name]["size"])
                and checksum in strong_checksums, "APT URI/checksum mismatch")
        decoded = urllib.parse.unquote(url)
        require(any(decoded == base + "/" + directory + "/" + name
                    for base, _ in REPOS[distribution]), "APT URI directory mismatch")
        found[name] = dict(url=url, **files[name])
    require(set(found) == set(files), "APT omitted source artifacts")
    return found


def check_vm():
    require(platform.system() == "Linux" and platform.machine() == "aarch64"
            and Path("/etc/cartridge-compat-vm").is_file(),
            "Run collection only in the marked ARM64 compatibility VM")


def scratch_path(path):
    require(path.is_absolute() and not path.is_symlink() and path.parent.is_dir(), "Expected absolute scratch path")
    require(path.resolve() == path and path != Path("/tmp") and Path("/tmp") in path.parents,
            "Use a nonsymlink directory below /tmp inside the VM")
    safe_path(path.relative_to("/tmp").as_posix())  # also safe inside the isolated APT configuration


def ensure_space(root, required, free=None):
    free = shutil.disk_usage(root).free if free is None else free
    require(free >= required + RESERVE,
            f"Insufficient space: need {required} bytes plus {RESERVE} reserve; free {free}")
    return free


def run(args, env=None):
    return subprocess.check_output(args, env=env, text=True, stderr=subprocess.STDOUT)


def dpkg_map(admindir=None, packages=()):
    args = ["dpkg-query"]
    if admindir:
        args += ["--admindir=" + str(admindir)]
    return run(args + ["-W", "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n", *packages])


def isolated_apt(root, distribution):
    apt = root / "apt" / distribution
    for name in ("etc/apt.conf.d", "etc/sources.list.d", "state/lists/partial", "cache/archives/partial", "log"):
        (apt / name).mkdir(parents=True)
    key = distribution + "-archive-keyring.gpg"
    shutil.copyfile("/usr/share/keyrings/" + key, apt / "etc/archive-keyring.gpg")
    (apt / "state/status").touch()
    (apt / "etc/sources.list").write_text("".join(
        f"deb-src [signed-by={apt}/etc/archive-keyring.gpg] {url} {suite} main\n"
        for url, suite in REPOS[distribution]))
    (apt / "apt.conf").write_text(f'''Dir "{apt}";
Dir::Etc "{apt}/etc";
Dir::Etc::main "apt.conf";
Dir::Etc::parts "apt.conf.d";
Dir::Etc::sourcelist "sources.list";
Dir::Etc::sourceparts "sources.list.d";
Dir::State "{apt}/state";
Dir::State::status "{apt}/state/status";
Dir::Cache "{apt}/cache";
Dir::Cache::pkgcache "";
Dir::Cache::srcpkgcache "";
Dir::Log "{apt}/log";
APT::Architecture "arm64";
APT::Architectures {{ "arm64"; }};
Acquire::Languages "none";
Acquire::GzipIndexes "true";
Acquire::Retries "2";
Acquire::https::Timeout "30";
APT::Update::Error-Mode "any";
''')
    env = dict(os.environ, APT_CONFIG=str(apt / "apt.conf"), LC_ALL="C")
    (apt / "update.log").write_text(run(["apt-get", "update"], env))
    releases = sorted((apt / "state/lists").glob("*_InRelease"))
    require(len(releases) == len(REPOS[distribution]), "Missing signed Release metadata")
    for release in releases:
        try:
            verification = run(["gpgv", "--status-fd=1", "--keyring", str(apt / "etc/archive-keyring.gpg"), str(release)])
        except subprocess.CalledProcessError as error:
            # A rollover Release can carry both trusted Bookworm signatures and
            # an extra signature absent from this keyring. APT already accepted
            # it; still require a valid signature from the supplied archive ring.
            require(error.returncode == 2 and "[GNUPG:] NO_PUBKEY " in error.output,
                    "Archive signature verification failed")
            verification = error.output
        require("[GNUPG:] VALIDSIG " in verification and not any(
            "[GNUPG:] " + status in verification for status in ("BADSIG ", "REVKEYSIG ", "EXPKEYSIG ", "EXPSIG ")),
            "No valid archive signature, or a bad/revoked/expired signature")
        release.with_name(release.name + ".gpgv.log").write_text(verification)
    return env


def snapshot(root, component, recipe):
    dest = root / "evidence"
    dest.mkdir()
    for name in ("component.json", "packages.tsv", "source-packages.tsv"):
        shutil.copyfile(regular(component / name), dest / name)
    mapping = dpkg_map(component / "rootfs/var/lib/dpkg")
    (dest / "binary-sources.tsv").write_text(mapping)
    binaries = parse_tsv(mapping, 4)
    packages = parse_tsv((dest / "packages.tsv").read_text(), 2)
    require(sorted((p, v) for p, v, _, _ in binaries) == sorted(packages),
            "Binary inventory differs from engine dpkg state")
    requested = parse_tsv((dest / "source-packages.tsv").read_text(), 2)
    require(set(requested) == {(p, v) for _, _, p, v in binaries}, "Engine source inventory differs from dpkg source mapping")
    static = dpkg_map(packages=STATIC_PACKAGES)
    (dest / "static-binary-sources.tsv").write_text(static)
    static_rows = parse_tsv(static, 4)
    require({b.split(":")[0] for b, _, _, _ in static_rows} == set(STATIC_PACKAGES)
            and {p for _, _, p, _ in static_rows} == {"glibc", "libcap2"}
            and len({(p, v) for _, _, p, v in static_rows}) == 2, "Unexpected static library sources")
    # Keep the actual installed static archives' hashes; no compilation or execution.
    static_inputs = ["/usr/lib/aarch64-linux-gnu/libc.a", "/usr/lib/aarch64-linux-gnu/libcap.a"]
    write_json(dest / "static-inputs.json", [dict(path=p, size=Path(p).stat().st_size, sha256=sha256(Path(p))) for p in static_inputs])
    shutil.copyfile(regular(recipe), dest / "build-arm64.sh")
    # Preserve the finalized notices and all installed Debian copyright files, including
    # doc aliases, resolving only links that remain within the finalized component.
    names = list((component / "notices").rglob("*"))
    names += list((component / "rootfs/usr/share/doc").rglob("copyright"))
    require(any(p.name == "copyright" for p in names), "Missing Debian copyright notices")
    for path in names:
        if path.is_dir():
            continue
        resolved = path.resolve(strict=True)
        require(resolved.is_relative_to(component), "Notice link escapes engine")
        regular(resolved)
        target = dest / "notices" / path.relative_to(component)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(resolved, target)
    for name in ("component.json", "packages.tsv", "source-packages.tsv"):
        require((component / name).read_bytes() == (dest / name).read_bytes(), "Engine inventory changed during snapshot")
    return {"debian": binaries, "ubuntu": static_rows}


def plan(component, output, recipe):
    check_vm()
    scratch_path(output)
    require(not output.exists(), "Output already exists; use collect/verify on its existing lock")
    require(component.is_dir() and not component.is_symlink(), "Expected finalized engine")
    component = component.resolve()
    require(not output.resolve().is_relative_to(component) and not component.is_relative_to(output.resolve()),
            "Source output and finalized engine must be disjoint")
    ensure_space(output.parent, RESERVE)  # room for bounded APT metadata before sizing payloads
    output.mkdir()
    mapping = snapshot(output, component, recipe)
    sources, artifacts, unresolved = [], {}, []
    for distribution, binaries in mapping.items():
        print("Authenticating " + distribution + " source indexes", flush=True)
        env = isolated_apt(output, distribution)
        for package, version in sorted({(p, v) for _, _, p, v in binaries}):
            try:
                text = run(["apt-cache", "showsrc", "--only-source", package], env)
                uris = run(["apt-get", "source", "--print-uris", "--only-source", "--download-only", package + "=" + version], env)
                fields, record = exact_source(text, package, version, uris, distribution)
                files = parse_uris(uris, distribution, fields)
            except (ValueError, subprocess.CalledProcessError) as error:
                unresolved.append(dict(distribution=distribution, package=package, version=version, error=str(error)))
                continue
            # The exact epoch remains in the source record and lock; filenames do not need colons.
            source_dir = safe_path("records/" + distribution + "/" + package + "/" + version.replace(":", "_"))
            record_path = output / source_dir
            record_path.mkdir(parents=True)
            (record_path / "Sources").write_text(record)
            (record_path / "uris.txt").write_text(uris)
            paths = []
            for name, item in sorted(files.items()):
                path = safe_path("artifacts/" + distribution + "/" + name)
                entry = dict(path=path, distribution=distribution, authentication="apt-signed-index", **item)
                require(path not in artifacts or artifacts[path] == entry, "Conflicting shared source artifact")
                artifacts[path] = entry
                paths.append(path)
            sources.append(dict(distribution=distribution, package=package, version=version,
                                binaries=[dict(package=b, version=v) for b, v, p, sv in binaries if (p, sv) == (package, version)],
                                record=source_dir + "/Sources", uris=source_dir + "/uris.txt", artifacts=paths))
    if unresolved:
        write_json(output / "unresolved.json", unresolved)
        raise ValueError(f"{len(unresolved)} exact source versions unresolved; no archive downloads or fallback (see unresolved.json)")
    artifacts["artifacts/upstream/bubblewrap-0.13.1.tar.xz"] = dict(
        path="artifacts/upstream/bubblewrap-0.13.1.tar.xz", size=BWRAP_SIZE, sha256=BWRAP_SHA256,
        url=BWRAP_URL, distribution="upstream", authentication="builder-pinned-sha256")
    evidence = [file_record(output, p) for p in sorted(output.rglob("*")) if p.is_file()]
    value = dict(schema=1, kind="browser-source-collection", created=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                 component_path=str(component), scope="exact distro source artifacts and pinned bubblewrap; no compliance assertion",
                 sources=sources, artifacts=sorted(artifacts.values(), key=lambda r: r["path"]), evidence=evidence,
                 source_bytes=sum(a["size"] for a in artifacts.values()), evidence_bytes=sum(e["size"] for e in evidence),
                 reserve_bytes=RESERVE)
    validate_index(output, value)
    write_json(output / LOCK, value)
    free = ensure_space(output, value["source_bytes"])
    print(json.dumps(dict(lock=str(output / LOCK), distro_sources=len(sources), source_bytes=value["source_bytes"],
                          evidence_bytes=value["evidence_bytes"], free_bytes=free, reserve_bytes=RESERVE), indent=2), flush=True)
    return value


def validate_index(root, value):
    require(isinstance(value, dict) and value.get("schema") == 1 and value.get("kind") == "browser-source-collection",
            "Unknown source index")
    require(set(value) >= {"artifacts", "evidence", "sources", "source_bytes", "evidence_bytes", "reserve_bytes"},
            "Incomplete source index")
    require(isinstance(value["sources"], list) and 0 < len(value["sources"]) <= 4096, "Invalid source count")
    artifacts, evidence = {}, {}
    for category, dest in (("artifacts", artifacts), ("evidence", evidence)):
        require(isinstance(value[category], list) and len(value[category]) <= 8192, "Invalid index count")
        for record in value[category]:
            require(isinstance(record, dict) and set(record) >= {"path", "size", "sha256"}, "Invalid index entry")
            path = safe_path(record["path"])
            require(path not in dest and type(record["size"]) is int and 0 <= record["size"] <= MAX_ARTIFACT
                    and isinstance(record["sha256"], str) and re.fullmatch(r"[a-f0-9]{64}", record["sha256"]),
                    "Invalid/duplicate index entry")
            within(root, path)
            require(path.startswith("artifacts/" if category == "artifacts" else ("evidence/", "records/", "apt/")),
                    "Incorrect index namespace")
            dest[path] = record
    require(0 < sum(a["size"] for a in artifacts.values()) == value["source_bytes"] <= MAX_TOTAL
            and sum(e["size"] for e in evidence.values()) == value["evidence_bytes"]
            and value["reserve_bytes"] == RESERVE, "Index size accounting mismatch")
    for record in evidence.values():
        check_file(root, record)
    expected = {}
    for name in ("component.json", "packages.tsv", "source-packages.tsv", "build-arm64.sh", "static-inputs.json"):
        require("evidence/" + name in evidence, "Missing locked engine/build evidence: " + name)
    require(any(p.startswith("evidence/notices/") for p in evidence), "Missing notices")
    for distro, name in (("debian", "binary-sources.tsv"), ("ubuntu", "static-binary-sources.tsv")):
        require("evidence/" + name in evidence, "Missing binary/source evidence")
        rows = parse_tsv(within(root, "evidence/" + name).read_text(), 4)
        for binary, binary_version, source, version in rows:
            expected.setdefault((distro, source, version), []).append(dict(package=binary, version=binary_version))
        if distro == "debian":
            packages = parse_tsv(within(root, "evidence/packages.tsv").read_text(), 2)
            require(sorted((b, v) for b, v, _, _ in rows) == sorted(packages), "Locked binary inventory mismatch")
    requested = parse_tsv(within(root, "evidence/source-packages.tsv").read_text(), 2)
    require({(p, v) for d, p, v in expected if d == "debian"} == set(requested), "Source inventory mapping mismatch")
    actual, used = set(), set()
    for source in value["sources"]:
        require(isinstance(source, dict) and set(source) ==
                {"distribution", "package", "version", "binaries", "record", "uris", "artifacts"}, "Invalid source entry")
        require(all(isinstance(source[k], str) for k in ("distribution", "package", "version", "record", "uris"))
                and isinstance(source["artifacts"], list) and all(isinstance(p, str) for p in source["artifacts"]),
                "Invalid source fields")
        key = (source["distribution"], source["package"], source["version"])
        require(key in expected and key not in actual and source["binaries"] == expected[key], "Altered/duplicate source version or mapping")
        actual.add(key)
        require(source["record"] in evidence and source["uris"] in evidence, "Missing locked source metadata")
        fields, _ = exact_source(within(root, source["record"]).read_text(), source["package"], source["version"])
        files = parse_uris(within(root, source["uris"]).read_text(), source["distribution"], fields)
        require(set(source["artifacts"]) == {"artifacts/" + source["distribution"] + "/" + n for n in files}
                and len(source["artifacts"]) == len(files), "Source artifact set incomplete")
        for path in source["artifacts"]:
            require(path in artifacts, "Missing source artifact entry")
            item = artifacts[path]
            require(item == dict(path=path, distribution=source["distribution"], authentication="apt-signed-index",
                                 **files[Path(path).name]), "Artifact differs from exact authenticated source metadata")
            used.add(path)
    require(actual == set(expected), "Incomplete exact source mapping")
    upstream = "artifacts/upstream/bubblewrap-0.13.1.tar.xz"
    require(artifacts.get(upstream) == dict(path=upstream, size=BWRAP_SIZE, sha256=BWRAP_SHA256, url=BWRAP_URL,
            distribution="upstream", authentication="builder-pinned-sha256"), "Bubblewrap pin changed")
    require(set(artifacts) == used | {upstream}, "Unmapped extra source artifact")
    return artifacts


class OfficialRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Distro artifacts never need arbitrary redirects. Bubblewrap is supplied locally.
        distribution = "debian" if urllib.parse.urlsplit(req.full_url).hostname in ("deb.debian.org", "security.debian.org") else "ubuntu"
        official_url(newurl, distribution)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def download(root, item, bubblewrap):
    target = within(root, item["path"])
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = within(root, item["path"] + ".partial")
    require(not partial.exists(), "Unverified partial exists; inspect/remove only that partial before retry")
    digest, size = hashlib.sha256(), 0
    try:
        if item["distribution"] == "upstream":
            require(bubblewrap is not None, "Supply the original pinned bubblewrap archive with --bubblewrap-archive")
            source = regular(bubblewrap).open("rb")
        else:
            source = urllib.request.build_opener(OfficialRedirects()).open(item["url"], timeout=90)
        with source, partial.open("xb") as dest:
            while chunk := source.read(1024 * 1024):
                size += len(chunk)
                require(size <= item["size"], "Download exceeds locked size")
                dest.write(chunk)
                digest.update(chunk)
            dest.flush()
            os.fsync(dest.fileno())
        require(size == item["size"] and digest.hexdigest() == item["sha256"], "Downloaded checksum/size mismatch")
        require(not target.exists(), "Refusing artifact replacement")
        partial.rename(target)
    except BaseException:
        if partial.exists() and not partial.is_symlink():
            partial.unlink()
        raise


def collect(root, bubblewrap):
    check_vm()
    scratch_path(root)
    value = strict_json(within(root, LOCK))
    artifacts = validate_index(root, value)
    remaining = []
    for item in artifacts.values():
        if within(root, item["path"]).exists():
            check_file(root, item)
        else:
            remaining.append(item)
    ensure_space(root, sum(item["size"] for item in remaining))
    if any(i["distribution"] == "upstream" for i in remaining):
        require(bubblewrap is not None and regular(bubblewrap).stat().st_size == BWRAP_SIZE
                and sha256(bubblewrap) == BWRAP_SHA256, "Original bubblewrap archive required with pinned size/checksum")
    for n, item in enumerate(remaining, 1):
        ensure_space(root, item["size"])
        print(f"[{n}/{len(remaining)}] {item['path']} ({item['size']} bytes)", flush=True)
        download(root, item, bubblewrap)
    verify(root, complete=False)
    checksums = [(r["path"], r["sha256"]) for r in value["evidence"] + value["artifacts"]]
    checksums.append((LOCK, sha256(root / LOCK)))
    sums = "".join(digest + "  " + name + "\n" for name, digest in sorted(checksums))
    sums_path = within(root, "SHA256SUMS")
    if sums_path.exists():
        require(regular(sums_path).read_text() == sums, "Completion checksum file changed")
    else:
        sums_path.write_text(sums)
    print(json.dumps(verify(root), indent=2), flush=True)


def verify(root, complete=True):
    value = strict_json(within(root, LOCK))
    validate_index(root, value)
    records = value["evidence"] + value["artifacts"]
    for record in value["artifacts"]:
        check_file(root, record)
    allowed = {r["path"] for r in records} | {LOCK, "SHA256SUMS"}
    for path in root.rglob("*"):
        require(not path.is_symlink(), "Unexpected symlink in bundle")
        if not path.is_dir():
            regular(path)
            require(path.relative_to(root).as_posix() in allowed, "Unlisted file in collection: " + str(path))
    if complete:
        expected = "".join(digest + "  " + name + "\n" for name, digest in sorted(
            [(r["path"], r["sha256"]) for r in records] + [(LOCK, sha256(root / LOCK))]))
        require(regular(within(root, "SHA256SUMS")).read_text() == expected, "Completion checksums differ")
    return dict(lock_sha256=sha256(root / LOCK), distro_sources=len(value["sources"]), artifacts=len(value["artifacts"]),
                source_bytes=value["source_bytes"], evidence_bytes=value["evidence_bytes"],
                collection_bytes=sum(p.stat().st_size for p in root.rglob("*") if p.is_file()),
                status="verified local collection; publication/compliance not asserted")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("plan", help="Authenticate indexes, lock exact URLs/sizes and refuse insufficient space")
    p.add_argument("--component", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--build-recipe", type=Path, required=True)
    for command in ("collect", "verify"):
        p = commands.add_parser(command)
        p.add_argument("--output", type=Path, required=True)
        if command == "collect":
            p.add_argument("--bubblewrap-archive", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "plan":
            plan(args.component, args.output, args.build_recipe)
        elif args.command == "collect":
            collect(args.output, args.bubblewrap_archive)
        else:
            print(json.dumps(verify(args.output), indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print("Source collection refused: " + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
