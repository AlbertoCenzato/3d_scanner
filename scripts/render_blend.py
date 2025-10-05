import bpy
import sys
from pathlib import Path
import json

# --- Parse command line arguments ---
argv = sys.argv
argv = argv[argv.index("--") + 1:] if "--" in argv else []

if len(argv) < 2:
    print("Usage: blender --background --python render_blend.py -- <blend_file> <output_dir>")
    sys.exit(1)

blend_file = Path(argv[0]).absolute()
output_dir = Path(argv[1]).absolute()
output_json = output_dir / "calibration.json"

# --- Load .blend file ---
print(f"Loading blend file: {blend_file}")
bpy.ops.wm.open_mainfile(filepath=str(blend_file))


# --- Export laser and camera position ---
target_names = {
    "Camera",
    "Area.001",
    "Area.002"
}

data = {}

for name in target_names:
    obj = bpy.data.objects.get(name)
    if obj is None:
        print(f"Warning: object '{name}' not found in the scene.")
        continue

    data[name] = {
        "location": list(obj.location),
        "rotation_euler": list(obj.rotation_euler),
        "scale": list(obj.scale)
    }

output_json.parent.mkdir(exist_ok=True)
with output_json.open("w", encoding="utf-8") as f:
    json.dump(data, f, indent=4)

print(f"Exported {len(data)} objects to {output_json}")

# --- Set render settings ---
scene = bpy.context.scene
scene.render.image_settings.file_format = 'PNG'
scene.render.filepath = str(output_dir / "frame_")

# Optional: set render engine and samples
scene.render.engine = 'CYCLES'  # or 'BLENDER_EEVEE'
scene.cycles.samples = 64

# --- Render animation ---
print(f"Rendering animation to directory: {output_dir}")
bpy.ops.render.render(animation=True)
print("Animation render complete.")
