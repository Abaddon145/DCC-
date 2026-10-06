"""Run only inside the disposable UE verification project, never a user project."""
import json
import hashlib
import struct
from pathlib import Path

import unreal


saved = Path(unreal.Paths.project_saved_dir())
probe = json.loads((saved / "stack-probe.json").read_text(encoding="utf-8"))
expected = {"StaticMesh", "Texture2D", "Material", "MaterialInstanceConstant"}
actual = {asset["className"] for asset in probe["assets"]}
if actual != expected:
    raise RuntimeError("Native fixture types differ: " + repr(actual))

results = []
for asset in probe["assets"]:
    path = saved / "StackThumbnails" / (asset["assetName"] + ".png")
    succeeded = unreal.StackThumbnailLibrary.export_thumbnail_png(asset["objectPath"], str(path), 256)
    result = {"objectPath": asset["objectPath"], "className": asset["className"], "succeeded": succeeded}
    if succeeded and path.is_file():
        header = path.read_bytes()[:24]
        result["png"] = header[:8] == b"\x89PNG\r\n\x1a\n"
        result["dimensions"] = list(struct.unpack(">II", header[16:24]))
        result["bytes"] = path.stat().st_size
        result["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    results.append(result)

(saved / "stack-thumbnail-probe.json").write_text(
    json.dumps({"engineVersion": unreal.SystemLibrary.get_engine_version(), "results": results}, indent=2),
    encoding="utf-8",
)
if not all(result.get("png") and result.get("dimensions") ==
           ([128, 128] if result["className"] == "Texture2D" else [256, 256]) for result in results):
    raise RuntimeError("UE native thumbnail verification failed; see Saved/stack-thumbnail-probe.json")
# The known fixture must contain a cube, a sphere and a texture, not four identical black exports.
if len({result["sha256"] for result in results}) < 3:
    raise RuntimeError("Native fixture thumbnails are identical or fallback images, not rendered assets")
unreal.log("STACK_NATIVE_THUMBNAILS_OK: four UE-generated PNGs with distinct native fixture images")
