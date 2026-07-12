import gzip
import io
import tarfile
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parent / "fixtures"
root.mkdir(parents=True, exist_ok=True)

data = b"hello archive\n"
(root / "hello.txt").write_bytes(data)
with gzip.open(root / "hello.txt.gz", "wb") as f:
    f.write(data)

buf = io.BytesIO()
with tarfile.open(fileobj=buf, mode="w:gz") as tar:
    info = tarfile.TarInfo(name="mypkg/package.json")
    payload = b'{"name":"mypkg"}'
    info.size = len(payload)
    tar.addfile(info, io.BytesIO(payload))
(root / "package.tar.gz").write_bytes(buf.getvalue())

zbuf = io.BytesIO()
with zipfile.ZipFile(zbuf, "w", compression=zipfile.ZIP_STORED) as zf:
    zf.writestr("bin/niao", b"fake-niao-binary")
    zf.writestr("bin/nm", b"fake-nm-binary")
(root / "release.zip").write_bytes(zbuf.getvalue())

zbuf2 = io.BytesIO()
with zipfile.ZipFile(zbuf2, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    zf.writestr("readme.txt", b"deflated zip fixture")
(root / "deflated.zip").write_bytes(zbuf2.getvalue())

print("wrote", sorted(p.name for p in root.iterdir()))
