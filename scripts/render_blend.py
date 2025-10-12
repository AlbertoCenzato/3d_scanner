import bpy
import sys
from pathlib import Path
import json
from math import degrees

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
    "camera",
    "laser_left",
    "laser_right"
}

data = {}

for name in target_names:
    obj = bpy.data.objects.get(name)
    if obj is None:
        print(f"Warning: object '{name}' not found in the scene.")
        continue

    obj_data = {}
    if name == "camera" and obj.type == 'CAMERA':
        cam_data = obj.data
        resolution_x_px = bpy.context.scene.render.resolution_x
        resolution_y_px = bpy.context.scene.render.resolution_y
        sensor_width_mm = cam_data.sensor_width
        obj_data["intrinsics"] = {
            "focal_length_m": cam_data.lens / 1000,
            "width_px": resolution_x_px,
            "height_px": resolution_y_px,
            "meters_per_px": (sensor_width_mm / resolution_x_px) / 1000
        }
        obj_data["extrinsics"] = {
            "rotation_euler_deg": [degrees(a) for a in obj.rotation_euler],
            "translation_m": list(obj.location)
        }
        obj_data["cam_2_img_plane_rotation_deg"] = [0.0,180.0,90.0]
    else:
        obj_data["translation_m"] = list(obj.location)
        obj_data["rotation_euler_deg"] = [degrees(a) for a  in obj.rotation_euler]

    data[name] = obj_data

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

